use super::oauth::{OAuthClient, OAuthConfig};
use super::remote_mcp::XRemoteMcp;
use super::token_store::TokenStore;
use super::{descriptor, parse_collect_options};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use rmcp::transport::async_rw::JsonRpcMessageCodec;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite};
use url::Url;

pub const DEFAULT_MCP_URL: &str = "https://api.x.com/mcp";
pub const DESCRIBE_TOOL: &str = "article_collector_describe_sources";
pub const COLLECT_TOOL: &str = "article_collector_collect";
pub const CONFIGURE_TOOL: &str = "article_collector_configure";
const MAX_MCP_FRAME_BYTES: usize = 16 * 1024 * 1024;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_CALLBACK_REQUEST_BYTES: usize = 16 * 1024;

#[async_trait]
pub trait OAuthConfigurator: Send + Sync {
    async fn configure(&self) -> Result<Value>;
}

pub struct StdioServer {
    mcp_url: Url,
    token_store: TokenStore,
    oauth: Option<OAuthClient>,
    configurator: Arc<dyn OAuthConfigurator>,
}

impl StdioServer {
    pub fn new(
        mcp_url: Url,
        token_store: TokenStore,
        oauth: Option<OAuthClient>,
        configurator: Arc<dyn OAuthConfigurator>,
    ) -> Self {
        Self {
            mcp_url,
            token_store,
            oauth,
            configurator,
        }
    }

    pub fn from_environment() -> Result<Self> {
        let mcp_url = Url::parse(DEFAULT_MCP_URL).context("Invalid default X MCP URL")?;
        let data_root = dirs::data_local_dir()
            .context("Unable to resolve the operating system user data directory")?
            .join("article-collector");
        let token_store = TokenStore::from_root(data_root);
        let oauth = std::env::var("X_OAUTH_CLIENT_ID")
            .ok()
            .map(OAuthConfig::from_client_id)
            .transpose()?
            .map(OAuthClient::new)
            .transpose()?;
        let configurator = Arc::new(InteractiveOAuthConfigurator {
            oauth: oauth.clone(),
            token_store: token_store.clone(),
        });
        Ok(Self::new(mcp_url, token_store, oauth, configurator))
    }

    pub async fn handle_message(&self, request: Value) -> Result<Option<Value>> {
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .context("MCP request is missing a method")?;
        if method == "notifications/initialized" {
            return Ok(None);
        }
        let Some(id) = request.get("id").cloned() else {
            return Ok(None);
        };
        if method == "initialize" {
            return Ok(Some(rpc_response(id, initialize_result())));
        }
        if method == "tools/list" {
            return Ok(Some(rpc_response(id, tools_list_result())));
        }
        let result = match method {
            "tools/call" => self.handle_tool_call(&request).await,
            _ => Err(HandlerError::new(-32601, "MCP method is not supported")),
        };
        Ok(Some(match result {
            Ok(result) => success_response(id, result),
            Err(error) => error_response(id, error.code, error.message),
        }))
    }

    async fn handle_tool_call(&self, request: &Value) -> std::result::Result<Value, HandlerError> {
        let params = request
            .get("params")
            .and_then(Value::as_object)
            .ok_or_else(|| HandlerError::invalid_params("tools/call params must be an object"))?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| HandlerError::invalid_params("tools/call name is required"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match name {
            DESCRIBE_TOOL => Ok(descriptor()),
            COLLECT_TOOL => self.collect(arguments).await,
            CONFIGURE_TOOL => {
                let result = self
                    .configurator
                    .configure()
                    .await
                    .map_err(HandlerError::from_anyhow)?;
                reject_secret_fields(&result).map_err(HandlerError::from_anyhow)?;
                Ok(result)
            }
            _ => Err(HandlerError::invalid_params(
                "tools/call name is not supported",
            )),
        }
    }

    async fn collect(&self, arguments: Value) -> std::result::Result<Value, HandlerError> {
        let request: CollectRequest = serde_json::from_value(arguments)
            .map_err(|_| HandlerError::invalid_params("collect arguments are invalid"))?;
        let options =
            parse_collect_options(&request.source, &request.options).map_err(|error| {
                HandlerError::invalid_params(sanitize_error_message(&error.to_string()))
            })?;
        let token = self
            .token_store
            .load()
            .map_err(HandlerError::from_anyhow)?
            .ok_or_else(|| {
                HandlerError::configuration("X is not configured; run plugin configure x")
            })?;
        let mut remote =
            XRemoteMcp::new(self.mcp_url.clone(), token).map_err(HandlerError::from_anyhow)?;
        if let Some(oauth) = &self.oauth {
            remote = remote.with_refresh(oauth.clone(), self.token_store.clone());
        }
        let items = remote
            .collect(&options, request.limit)
            .await
            .map_err(HandlerError::from_anyhow)?;
        Ok(json!({"items": items}))
    }
}

pub async fn run_stdio() -> Result<()> {
    let server = StdioServer::from_environment()?;
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = FramedRead::new(
        stdin,
        JsonRpcMessageCodec::<Value>::new_with_max_length(MAX_MCP_FRAME_BYTES),
    );
    let mut writer = FramedWrite::new(
        stdout,
        JsonRpcMessageCodec::<Value>::new_with_max_length(MAX_MCP_FRAME_BYTES),
    );
    while let Some(frame) = reader.next().await {
        let request = frame.context("Invalid or oversized plugin MCP request")?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let response = match server.handle_message(request).await {
            Ok(response) => response,
            Err(_error) => Some(error_response(
                id,
                -32000,
                "X connector request failed".to_string(),
            )),
        };
        if let Some(response) = response {
            writer
                .send(response)
                .await
                .context("Failed to write plugin MCP response")?;
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectRequest {
    source: String,
    limit: usize,
    #[serde(default)]
    options: Map<String, Value>,
}

#[derive(Debug)]
struct HandlerError {
    code: i64,
    message: String,
}

impl HandlerError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }

    fn configuration(message: impl Into<String>) -> Self {
        Self::new(-32000, message)
    }

    fn from_anyhow(error: anyhow::Error) -> Self {
        Self::configuration(sanitize_error_message(&error.to_string()))
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "article-collector-x", "version": env!("CARGO_PKG_VERSION")}
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            {"name": DESCRIBE_TOOL, "description": "Describe the X recommendation source", "inputSchema": {"type": "object", "additionalProperties": false}},
            {"name": COLLECT_TOOL, "description": "Collect X recommendation candidates", "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["source", "limit"],
                "properties": {
                    "source": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 0},
                    "options": {"type": "object"}
                }
            }},
            {"name": CONFIGURE_TOOL, "description": "Configure X OAuth", "inputSchema": {"type": "object", "additionalProperties": false}}
        ]
    })
}

fn success_response(id: Value, structured_content: Value) -> Value {
    rpc_response(
        id,
        json!({
            "content": [{"type": "text", "text": serde_json::to_string(&structured_content).unwrap_or_else(|_| "{}".to_string())}],
            "structuredContent": structured_content,
            "isError": false
        }),
    )
}

fn rpc_response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn reject_secret_fields(value: &Value) -> Result<()> {
    const SECRET_PARTS: [&str; 4] = ["token", "secret", "authorization", "code"];
    if let Some(object) = value.as_object() {
        for (key, nested) in object {
            let lowercase = key.to_ascii_lowercase();
            if SECRET_PARTS.iter().any(|part| lowercase.contains(part)) {
                bail!("configuration response contains a forbidden secret-like field")
            }
            reject_secret_fields(nested)?;
        }
    } else if let Some(array) = value.as_array() {
        for nested in array {
            reject_secret_fields(nested)?;
        }
    }
    Ok(())
}

fn sanitize_error_message(message: &str) -> String {
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

struct InteractiveOAuthConfigurator {
    oauth: Option<OAuthClient>,
    token_store: TokenStore,
}

#[async_trait]
impl OAuthConfigurator for InteractiveOAuthConfigurator {
    async fn configure(&self) -> Result<Value> {
        let oauth = self
            .oauth
            .as_ref()
            .context("X_OAUTH_CLIENT_ID is required; set it before plugin configure x")?;
        let listener = TcpListener::bind("127.0.0.1:53681")
            .await
            .context("Failed to bind X OAuth loopback callback")?;
        let authorization = oauth.authorization_request()?;
        eprintln!("Open this URL to authorize X: {}", authorization.url);
        let callback = wait_for_callback(listener).await?;
        let code = oauth
            .parse_callback(&callback, &authorization.state)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let token = oauth.exchange_code(&code, &authorization.verifier).await?;
        self.token_store.save(&token)?;
        Ok(json!({"status": "configured", "message": "X OAuth configuration saved"}))
    }
}

async fn wait_for_callback(listener: TcpListener) -> Result<Url> {
    tokio::time::timeout(CALLBACK_TIMEOUT, async move {
        loop {
            let (mut stream, address) = listener.accept().await?;
            if !address.ip().is_loopback() {
                continue;
            }
            let target = read_callback_target(&mut stream).await?;
            let callback = Url::parse(&format!("http://127.0.0.1:53681{target}"))?;
            let valid_path = callback.path() == "/callback";
            write_callback_response(&mut stream, valid_path).await?;
            if valid_path {
                return Ok(callback);
            }
        }
    })
    .await
    .context("X OAuth callback timed out")?
}

async fn read_callback_target(stream: &mut TcpStream) -> Result<String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    while bytes.len() < MAX_CALLBACK_REQUEST_BYTES {
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if bytes.len() >= MAX_CALLBACK_REQUEST_BYTES {
        bail!("X OAuth callback request is too large")
    }
    let line = std::str::from_utf8(&bytes)?
        .lines()
        .next()
        .context("X OAuth callback has no request line")?;
    let mut fields = line.split_whitespace();
    if fields.next() != Some("GET") {
        bail!("X OAuth callback must use GET")
    }
    Ok(fields
        .next()
        .context("X OAuth callback has no request target")?
        .to_string())
}

async fn write_callback_response(stream: &mut TcpStream, success: bool) -> Result<()> {
    let (status, body) = if success {
        (
            "200 OK",
            "Authorization complete. You can return to article-collector.",
        )
    } else {
        ("400 Bad Request", "Invalid OAuth callback.")
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}
