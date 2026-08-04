use super::oauth::OAuthClient;
use super::token_store::{TokenSet, TokenStore};
use super::{normalize_response, CollectOptions, CollectionMode, PluginCandidate};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use rmcp::model::{CallToolRequestParams, ClientInfo, ProtocolVersion, Tool};
use rmcp::transport::{
    streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
};
use rmcp::{RoleClient, ServiceError, ServiceExt};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use url::Url;

const SEARCH_TOOL: &str = "searchPostsRecent";
const ME_TOOL: &str = "getUsersMe";
const TIMELINE_TOOL: &str = "getUsersTimeline";
const BOOKMARKS_TOOL: &str = "getUsersBookmarks";
const MAX_RESULTS_PER_PAGE: usize = 100;
const MAX_PAGE_COUNT: usize = 100;
const TWEET_FIELDS: &str = "author_id,created_at,public_metrics";
const USER_FIELDS: &str = "name,username";

#[derive(Debug, Clone)]
pub struct XRemoteMcp {
    mcp_url: Url,
    token: TokenSet,
    refresh: Option<RefreshConfig>,
}

#[derive(Debug, Clone)]
struct RefreshConfig {
    oauth: OAuthClient,
    store: TokenStore,
}

#[derive(Debug)]
enum RemoteFailure {
    Unauthorized,
    InsufficientScope,
    RateLimited,
    ToolMismatch(&'static str),
    InvalidResponse,
    Transport,
    Configuration(&'static str),
    Refresh,
}

impl std::fmt::Display for RemoteFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => formatter.write_str("X MCP authentication is required"),
            Self::InsufficientScope => formatter.write_str(
                "X MCP denied the request because the configured token lacks a required read scope",
            ),
            Self::RateLimited => formatter.write_str("X MCP rate limit exceeded"),
            Self::ToolMismatch(tool) => {
                write!(formatter, "X MCP required read tool is unavailable: {tool}")
            }
            Self::InvalidResponse => formatter.write_str("X MCP returned a malformed response"),
            Self::Transport => formatter.write_str("X MCP transport request failed"),
            Self::Configuration(message) => formatter.write_str(message),
            Self::Refresh => formatter.write_str("X OAuth token refresh failed"),
        }
    }
}

impl std::error::Error for RemoteFailure {}

impl XRemoteMcp {
    pub fn new(mcp_url: Url, token: TokenSet) -> Result<Self> {
        if !matches!(mcp_url.scheme(), "http" | "https") || mcp_url.host_str().is_none() {
            bail!("X MCP endpoint must be an HTTP(S) URL")
        }
        if token.access_token.trim().is_empty() {
            bail!("X MCP access token is missing")
        }
        Ok(Self {
            mcp_url,
            token,
            refresh: None,
        })
    }

    pub fn with_refresh(mut self, oauth: OAuthClient, store: TokenStore) -> Self {
        self.refresh = Some(RefreshConfig { oauth, store });
        self
    }

    pub async fn validate_tools(&self) -> Result<()> {
        let mut token = self.token.clone();
        for attempt in 0..=1 {
            if token.is_expired(Utc::now()) {
                token = self.refresh_token(&token).await?;
            }
            match self.validate_tools_once(&token).await {
                Ok(()) => return Ok(()),
                Err(RemoteFailure::Unauthorized) if attempt == 0 => {
                    token = self.refresh_token(&token).await?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("the bounded X MCP authentication retry must return")
    }

    pub async fn collect(
        &self,
        options: &CollectOptions,
        limit: usize,
    ) -> Result<Vec<PluginCandidate>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut token = self.token.clone();
        for attempt in 0..=1 {
            if token.is_expired(Utc::now()) {
                token = self.refresh_token(&token).await?;
            }
            match self.collect_once(&token, options, limit).await {
                Ok(items) => return Ok(items),
                Err(RemoteFailure::Unauthorized) if attempt == 0 => {
                    token = self.refresh_token(&token).await?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("the bounded X MCP authentication retry must return")
    }

    async fn refresh_token(&self, current: &TokenSet) -> Result<TokenSet> {
        let refresh = self
            .refresh
            .as_ref()
            .context("X OAuth token is expired or rejected; run plugin configure x")?;
        let refresh_token = current
            .refresh_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .context("X OAuth refresh token is missing; run plugin configure x")?;
        let token = refresh
            .oauth
            .refresh(refresh_token)
            .await
            .map_err(|_| RemoteFailure::Refresh)?;
        refresh
            .store
            .save(&token)
            .map_err(|_| RemoteFailure::Refresh)?;
        Ok(token)
    }

    async fn validate_tools_once(&self, token: &TokenSet) -> Result<(), RemoteFailure> {
        let client = self.connect(token).await?;
        let result = async {
            let tools = client
                .peer()
                .list_all_tools()
                .await
                .map_err(map_service_error)?;
            validate_tool_list(&tools)
        }
        .await;
        let _ = client.cancel().await;
        result
    }

    async fn collect_once(
        &self,
        token: &TokenSet,
        options: &CollectOptions,
        limit: usize,
    ) -> Result<Vec<PluginCandidate>, RemoteFailure> {
        let client = self.connect(token).await?;
        let result = async {
            let tools = client
                .peer()
                .list_all_tools()
                .await
                .map_err(map_service_error)?;
            validate_tool_list(&tools)?;

            match options.mode {
                CollectionMode::Search => {
                    let query = options
                        .query
                        .as_deref()
                        .ok_or(RemoteFailure::Configuration(
                            "X search mode requires a query",
                        ))?;
                    collect_posts(client.peer(), PostCall::Search(query.to_string()), limit).await
                }
                CollectionMode::Timeline | CollectionMode::Bookmarks => {
                    let user = call_tool(client.peer(), ME_TOOL, user_fields_arguments()).await?;
                    let user_id = user
                        .get("data")
                        .and_then(|data| data.get("id"))
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or(RemoteFailure::InvalidResponse)?;
                    let call = match options.mode {
                        CollectionMode::Timeline => PostCall::Timeline(user_id.to_string()),
                        CollectionMode::Bookmarks => PostCall::Bookmarks(user_id.to_string()),
                        CollectionMode::Search => unreachable!(),
                    };
                    collect_posts(client.peer(), call, limit).await
                }
            }
        }
        .await;
        let _ = client.cancel().await;
        result
    }

    async fn connect(
        &self,
        token: &TokenSet,
    ) -> Result<rmcp::service::RunningService<RoleClient, ClientInfo>, RemoteFailure> {
        let mut config =
            StreamableHttpClientTransportConfig::with_uri(self.mcp_url.as_str().to_string())
                .auth_header(token.access_token.clone());
        config.allow_stateless = true;
        config.reinit_on_expired_session = true;
        let transport = StreamableHttpClientTransport::from_config(config);
        ClientInfo::default()
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .serve(transport)
            .await
            .map_err(map_initialize_error)
    }
}

#[derive(Debug, Clone)]
enum PostCall {
    Search(String),
    Timeline(String),
    Bookmarks(String),
}

async fn collect_posts(
    client: &rmcp::Peer<RoleClient>,
    call: PostCall,
    limit: usize,
) -> Result<Vec<PluginCandidate>, RemoteFailure> {
    let mut candidates = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut next_token = None;
    for page in 0..MAX_PAGE_COUNT {
        let remaining = limit.saturating_sub(candidates.len());
        if remaining == 0 {
            break;
        }
        let arguments = match &call {
            PostCall::Search(query) => post_fields_arguments(next_token.as_deref(), remaining)
                .into_iter()
                .chain([(String::from("query"), json!(query))])
                .collect(),
            PostCall::Timeline(user_id) | PostCall::Bookmarks(user_id) => {
                post_fields_arguments(next_token.as_deref(), remaining)
                    .into_iter()
                    .chain([(String::from("id"), json!(user_id))])
                    .collect()
            }
        };
        let tool = match &call {
            PostCall::Search(_) => SEARCH_TOOL,
            PostCall::Timeline(_) => TIMELINE_TOOL,
            PostCall::Bookmarks(_) => BOOKMARKS_TOOL,
        };
        let response = call_tool(client, tool, arguments).await?;
        let page_candidates =
            normalize_response(&response).map_err(|_| RemoteFailure::InvalidResponse)?;
        for candidate in page_candidates {
            let id = candidate
                .metadata
                .get("tweet_id")
                .and_then(Value::as_str)
                .ok_or(RemoteFailure::InvalidResponse)?;
            if seen_ids.insert(id.to_string()) {
                candidates.push(candidate);
                if candidates.len() == limit {
                    break;
                }
            }
        }

        if candidates.len() == limit {
            break;
        }
        let next = response
            .get("meta")
            .and_then(|meta| meta.get("next_token"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned);
        if next == next_token {
            return Err(RemoteFailure::InvalidResponse);
        }
        next_token = next;
        if next_token.is_none() {
            break;
        }
        if page + 1 == MAX_PAGE_COUNT {
            return Err(RemoteFailure::InvalidResponse);
        }
    }
    Ok(candidates)
}

fn post_fields_arguments(next_token: Option<&str>, remaining: usize) -> Map<String, Value> {
    let mut arguments = Map::new();
    arguments.insert(
        "max_results".to_string(),
        json!(remaining.min(MAX_RESULTS_PER_PAGE)),
    );
    arguments.insert("expansions".to_string(), json!("author_id"));
    arguments.insert("tweet.fields".to_string(), json!(TWEET_FIELDS));
    arguments.insert("user.fields".to_string(), json!(USER_FIELDS));
    if let Some(next_token) = next_token {
        arguments.insert("next_token".to_string(), json!(next_token));
    }
    arguments
}

fn user_fields_arguments() -> Map<String, Value> {
    Map::from_iter([(String::from("user.fields"), json!(USER_FIELDS))])
}

async fn call_tool(
    client: &rmcp::Peer<RoleClient>,
    name: &str,
    arguments: Map<String, Value>,
) -> Result<Value, RemoteFailure> {
    let result = client
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
        .await
        .map_err(map_service_error)?;
    if result.is_error == Some(true) {
        return Err(RemoteFailure::InvalidResponse);
    }
    result
        .into_typed::<Value>()
        .map_err(|_| RemoteFailure::InvalidResponse)
}

fn validate_tool_list(tools: &[Tool]) -> Result<(), RemoteFailure> {
    for (required, properties) in [
        (SEARCH_TOOL, ["query", "max_results"].as_slice()),
        (ME_TOOL, ["user.fields"].as_slice()),
        (TIMELINE_TOOL, ["id", "max_results"].as_slice()),
        (BOOKMARKS_TOOL, ["id", "max_results"].as_slice()),
    ] {
        let Some(tool) = tools.iter().find(|tool| tool.name == required) else {
            return Err(RemoteFailure::ToolMismatch(required));
        };
        if tool.input_schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err(RemoteFailure::ToolMismatch(required));
        }
        let Some(schema_properties) = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
        else {
            return Err(RemoteFailure::ToolMismatch(required));
        };
        if properties
            .iter()
            .any(|property| !schema_properties.contains_key(*property))
        {
            return Err(RemoteFailure::ToolMismatch(required));
        }
    }
    Ok(())
}

fn map_initialize_error(error: rmcp::service::ClientInitializeError) -> RemoteFailure {
    classify_error_message(&error.to_string())
}

fn map_service_error(error: ServiceError) -> RemoteFailure {
    classify_error_message(&error.to_string())
}

fn classify_error_message(message: &str) -> RemoteFailure {
    let lower = message.to_ascii_lowercase();
    if lower.contains("auth required") || lower.contains("http 401") {
        RemoteFailure::Unauthorized
    } else if lower.contains("insufficient scope") || lower.contains("http 403") {
        RemoteFailure::InsufficientScope
    } else if lower.contains("http 429") {
        RemoteFailure::RateLimited
    } else {
        RemoteFailure::Transport
    }
}
