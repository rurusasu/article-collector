use crate::plugins::installer::{validate_descriptor, PluginProbe};
use crate::plugins::types::{
    CollectRequest, CollectResponse, InstalledPlugin, PluginCandidate, PluginDescriptor,
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use rmcp::transport::async_rw::JsonRpcMessageCodec;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite};

pub const MAX_MCP_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_TITLE_BYTES: usize = 8 * 1024;
const MAX_CONTENT_BYTES: usize = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 256 * 1024;
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
const COLLECT_TIMEOUT: Duration = Duration::from_secs(60);
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(300);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

const DESCRIBE_TOOL: &str = "article_collector_describe_sources";
const COLLECT_TOOL: &str = "article_collector_collect";
const CONFIGURE_TOOL: &str = "article_collector_configure";

#[derive(Debug, Clone, Default)]
pub struct PluginRuntime;

impl PluginRuntime {
    pub async fn describe(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        let mut session = McpSession::start(plugin).await?;
        let result = session
            .call_tool(DESCRIBE_TOOL, json!({}), COLLECT_TIMEOUT)
            .await;
        let close_result = session.close().await;
        let descriptor: PluginDescriptor =
            serde_json::from_value(structured_content(&result?)?.clone())
                .context("Invalid structured describe_sources response")?;
        close_result?;
        Ok(descriptor)
    }

    pub async fn collect(
        &self,
        plugin: &InstalledPlugin,
        request: CollectRequest,
    ) -> Result<Vec<PluginCandidate>> {
        let maximum = request.limit;
        let mut session = McpSession::start(plugin).await?;
        let arguments =
            serde_json::to_value(request).context("Failed to encode collect request")?;
        let result = session
            .call_tool(COLLECT_TOOL, arguments, COLLECT_TIMEOUT)
            .await;
        let close_result = session.close().await;
        let response: CollectResponse =
            serde_json::from_value(structured_content(&result?)?.clone())
                .context("Invalid structured collect response")?;
        validate_collect_response(maximum, &response)?;
        close_result?;
        Ok(response.items)
    }

    pub async fn configure(&self, plugin: &InstalledPlugin) -> Result<Value> {
        if !plugin.manifest.supports_configuration {
            bail!("Plugin '{}' does not support configuration", plugin.name);
        }
        let mut session = McpSession::start(plugin).await?;
        let result = session
            .call_tool(CONFIGURE_TOOL, json!({}), CONFIGURE_TIMEOUT)
            .await;
        let close_result = session.close().await;
        let value = structured_content(&result?)?.clone();
        reject_secret_fields(&value)?;
        close_result?;
        Ok(value)
    }

    pub async fn doctor(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        let descriptor = self.describe(plugin).await?;
        validate_descriptor(&plugin.manifest, &descriptor)?;
        Ok(descriptor)
    }
}

#[async_trait]
impl PluginProbe for PluginRuntime {
    async fn verify(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        self.describe(plugin).await
    }
}

struct McpSession {
    child: Child,
    reader: FramedRead<ChildStdout, JsonRpcMessageCodec<Value>>,
    writer: FramedWrite<ChildStdin, JsonRpcMessageCodec<Value>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    next_id: u64,
}

impl McpSession {
    async fn start(plugin: &InstalledPlugin) -> Result<Self> {
        let executable = plugin.executable.canonicalize().with_context(|| {
            format!(
                "Failed to resolve plugin executable {}",
                plugin.executable.display()
            )
        })?;
        if !executable.is_file() {
            bail!("Plugin executable is not a file: {}", executable.display());
        }
        let mut command = Command::new(&executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear();
        copy_allowed_environment(&mut command, &plugin.manifest.environment)?;
        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to start plugin {}", plugin.name))?;
        let stdin = child.stdin.take().context("Plugin stdin is unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("Plugin stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("Plugin stderr is unavailable")?;
        let stderr_buffer = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(capture_stderr(stderr, stderr_buffer.clone()));

        let mut session = Self {
            child,
            reader: FramedRead::new(
                stdout,
                JsonRpcMessageCodec::new_with_max_length(MAX_MCP_FRAME_BYTES),
            ),
            writer: FramedWrite::new(
                stdin,
                JsonRpcMessageCodec::new_with_max_length(MAX_MCP_FRAME_BYTES),
            ),
            stderr: stderr_buffer,
            next_id: 1,
        };
        let initialize = json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {
                "name": "article-collector",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        tokio::time::timeout(
            INITIALIZE_TIMEOUT,
            session.request("initialize", initialize),
        )
        .await
        .context("Plugin MCP initialize timed out")??;
        session
            .writer
            .send(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .await
            .context("Failed to send MCP initialized notification")?;
        let tools = session
            .request("tools/list", json!({}))
            .await
            .context("Failed to list plugin MCP tools")?;
        let names = tools
            .get("tools")
            .and_then(Value::as_array)
            .context("MCP tools/list response has no tools array")?
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str));
        validate_required_tools(names)?;
        Ok(session)
    }

    async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        tokio::time::timeout(
            timeout,
            self.request(
                "tools/call",
                json!({
                    "name": name,
                    "arguments": arguments
                }),
            ),
        )
        .await
        .with_context(|| format!("Plugin MCP tool '{name}' timed out"))?
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.writer
            .send(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }))
            .await
            .with_context(|| format!("Failed to send MCP request '{method}'"))?;
        while let Some(frame) = self.reader.next().await {
            let frame = frame
                .with_context(|| format!("Invalid or oversized MCP response for '{method}'"))?;
            if frame.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = frame.get("error") {
                bail!("Plugin MCP request '{method}' failed: {error}");
            }
            return frame
                .get("result")
                .cloned()
                .with_context(|| format!("MCP response for '{method}' has no result"));
        }
        let stderr = sanitized_stderr(&self.stderr).await;
        bail!("Plugin exited before responding to '{method}'{stderr}")
    }

    async fn close(mut self) -> Result<()> {
        self.writer
            .close()
            .await
            .context("Failed to close plugin stdin")?;
        match tokio::time::timeout(SHUTDOWN_TIMEOUT, self.child.wait()).await {
            Ok(status) => {
                let status = status.context("Failed to wait for plugin process")?;
                if !status.success() {
                    let stderr = sanitized_stderr(&self.stderr).await;
                    bail!("Plugin exited abnormally with {status}{stderr}");
                }
            }
            Err(_) => {
                self.child
                    .kill()
                    .await
                    .context("Failed to kill plugin process")?;
                let _ = self.child.wait().await;
            }
        }
        Ok(())
    }
}

pub fn validate_collect_response(maximum: usize, response: &CollectResponse) -> Result<()> {
    if response.items.len() > maximum {
        bail!(
            "Plugin returned more items ({}) than requested ({maximum})",
            response.items.len()
        );
    }
    for (index, item) in response.items.iter().enumerate() {
        if !matches!(item.url.scheme(), "http" | "https") {
            bail!("Plugin candidate {index} URL must use HTTP or HTTPS");
        }
        if item.title.len() > MAX_TITLE_BYTES {
            bail!("Plugin candidate {index} title exceeds {MAX_TITLE_BYTES} bytes");
        }
        if item.content.len() > MAX_CONTENT_BYTES {
            bail!("Plugin candidate {index} content exceeds {MAX_CONTENT_BYTES} bytes");
        }
        for field in ["source", "site", "rank"] {
            if item.metadata.contains_key(field) {
                bail!("Plugin candidate contains reserved metadata field '{field}'");
            }
        }
    }
    Ok(())
}

pub fn validate_required_tools<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let names = names.into_iter().collect::<HashSet<_>>();
    for required in [DESCRIBE_TOOL, COLLECT_TOOL] {
        if !names.contains(required) {
            bail!("Plugin MCP server is missing required tool '{required}'");
        }
    }
    Ok(())
}

pub fn structured_content(result: &Value) -> Result<&Value> {
    result
        .get("structuredContent")
        .context("Plugin MCP tool response must include structuredContent")
}

fn copy_allowed_environment(command: &mut Command, allowed: &[String]) -> Result<()> {
    #[cfg(windows)]
    if let Some(value) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", value);
    }
    for name in allowed {
        if !valid_environment_name(name) {
            bail!("Invalid plugin environment variable name '{name}'");
        }
        if matches!(
            name.as_str(),
            "LD_PRELOAD" | "LD_LIBRARY_PATH" | "DYLD_INSERT_LIBRARIES" | "DYLD_LIBRARY_PATH"
        ) {
            bail!("Plugin environment variable '{name}' is not allowed");
        }
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    Ok(())
}

fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first == b'_' || first.is_ascii_uppercase())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

async fn capture_stderr(mut stderr: tokio::process::ChildStderr, buffer: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = [0_u8; 8192];
    while let Ok(count) = stderr.read(&mut chunk).await {
        if count == 0 {
            break;
        }
        let mut buffer = buffer.lock().await;
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.len() > MAX_STDERR_BYTES {
            let excess = buffer.len() - MAX_STDERR_BYTES;
            buffer.drain(..excess);
        }
    }
}

async fn sanitized_stderr(buffer: &Arc<Mutex<Vec<u8>>>) -> String {
    let buffer = buffer.lock().await;
    if buffer.is_empty() {
        String::new()
    } else {
        format!(
            "; stderr tail: {}",
            String::from_utf8_lossy(&buffer).replace(['\r', '\n'], " ")
        )
    }
}

fn reject_secret_fields(value: &Value) -> Result<()> {
    const SECRET_PARTS: [&str; 4] = ["token", "secret", "authorization", "code"];
    if let Some(object) = value.as_object() {
        for (key, nested) in object {
            let lowercase = key.to_ascii_lowercase();
            if SECRET_PARTS.iter().any(|part| lowercase.contains(part)) {
                bail!("Plugin configure response contains forbidden secret-like field '{key}'");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::types::{CollectResponse, PluginCandidate};
    use rmcp::transport::async_rw::JsonRpcMessageCodec;
    use serde_json::{json, Map, Value};
    use tokio_util::bytes::BytesMut;
    use tokio_util::codec::Decoder;
    use url::Url;

    fn candidate() -> PluginCandidate {
        PluginCandidate {
            title: "candidate".to_string(),
            url: Url::parse("https://example.com/article").unwrap(),
            content: "body".to_string(),
            metadata: Map::new(),
        }
    }

    /// 検証: plugin metadataはcore所有fieldを偽装できない
    /// 理由: source/site/rankの所有権をcoreに限定するため
    /// リスク: pluginが履歴・表示上のsource attributionを書き換える
    #[test]
    fn collect_rejects_reserved_metadata() {
        for field in ["source", "site", "rank"] {
            let mut item = candidate();
            item.metadata.insert(field.to_string(), json!("spoofed"));
            let error =
                validate_collect_response(1, &CollectResponse { items: vec![item] }).unwrap_err();
            assert!(error
                .to_string()
                .contains(&format!("reserved metadata field '{field}'")));
        }
    }

    /// 検証: response件数・URL scheme・title/content byte上限を強制する
    /// 理由: plugin processを信頼境界外としてメモリと下流inputを制限するため
    /// リスク: 過大responseや危険schemeが既存pipelineへ入る
    #[test]
    fn collect_rejects_semantically_invalid_candidates() {
        let too_many = CollectResponse {
            items: vec![candidate(), candidate()],
        };
        assert!(validate_collect_response(1, &too_many)
            .unwrap_err()
            .to_string()
            .contains("more items"));

        let mut invalid_url = candidate();
        invalid_url.url = Url::parse("ftp://example.com/file").unwrap();
        assert!(validate_collect_response(
            1,
            &CollectResponse {
                items: vec![invalid_url]
            }
        )
        .unwrap_err()
        .to_string()
        .contains("HTTP"));

        let mut long_title = candidate();
        long_title.title = "t".repeat(8 * 1024 + 1);
        assert!(validate_collect_response(
            1,
            &CollectResponse {
                items: vec![long_title]
            }
        )
        .unwrap_err()
        .to_string()
        .contains("title"));

        let mut long_content = candidate();
        long_content.content = "c".repeat(1024 * 1024 + 1);
        assert!(validate_collect_response(
            1,
            &CollectResponse {
                items: vec![long_content]
            }
        )
        .unwrap_err()
        .to_string()
        .contains("content"));
    }

    /// 検証: MCP codecは16MiBを超えるnewlineなしframeを拒否する
    /// 理由: child stdoutでunbounded allocationを起こさないため
    /// リスク: 悪意あるpluginがcoreをOOMにする
    #[test]
    fn mcp_codec_rejects_oversized_frame() {
        let mut codec = JsonRpcMessageCodec::<Value>::new_with_max_length(MAX_MCP_FRAME_BYTES);
        let mut bytes = BytesMut::from(&vec![b'a'; MAX_MCP_FRAME_BYTES + 1][..]);

        let error = codec.decode(&mut bytes).unwrap_err();

        assert!(error.to_string().contains("max line length"));
    }

    #[test]
    fn requires_describe_and_collect_tools_and_structured_content() {
        assert!(
            validate_required_tools(["article_collector_describe_sources"])
                .unwrap_err()
                .to_string()
                .contains("article_collector_collect")
        );
        let error =
            structured_content(&json!({"content": [{"type": "text", "text": "{}"}]})).unwrap_err();
        assert!(error.to_string().contains("structuredContent"));
    }
}
