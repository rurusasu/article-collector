pub mod normalize;
pub mod oauth;
pub mod remote_mcp;
pub mod stdio;
pub mod token_store;
pub use normalize::normalize_response;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};
use url::Url;

pub const PLUGIN_NAME: &str = "x";
pub const SOURCE_NAME: &str = "twitter";
pub const PROTOCOL_VERSION: &str = "1.0.0";
pub const MAX_TITLE_CHARS: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionMode {
    Search,
    Timeline,
    Bookmarks,
}

impl CollectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Timeline => "timeline",
            Self::Bookmarks => "bookmarks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectOptions {
    pub mode: CollectionMode,
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PluginCandidate {
    pub title: String,
    pub url: Url,
    pub content: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

pub fn descriptor() -> Value {
    json!({
        "protocol_version": PROTOCOL_VERSION,
        "name": PLUGIN_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "sources": [{
            "name": SOURCE_NAME,
            "aliases": ["x", "x-twitter"],
            "options_schema": options_schema()
        }],
        "supports_configuration": true
    })
}

pub fn options_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["mode"],
        "properties": {
            "mode": {"type": "string", "enum": ["search", "timeline", "bookmarks"]},
            "query": {"type": "string"}
        },
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode", "query"],
                "properties": {
                    "mode": {"const": "search"},
                    "query": {"type": "string", "minLength": 1}
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode"],
                "properties": {"mode": {"const": "timeline"}}
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode"],
                "properties": {"mode": {"const": "bookmarks"}}
            }
        ]
    })
}

pub fn parse_collect_options(source: &str, options: &Map<String, Value>) -> Result<CollectOptions> {
    if !matches!(
        source.to_ascii_lowercase().as_str(),
        "twitter" | "x" | "x-twitter"
    ) {
        bail!("Unsupported X connector source '{source}'")
    }
    for key in options.keys() {
        if !matches!(key.as_str(), "mode" | "query") {
            bail!("Unknown X connector option '{key}'")
        }
    }

    let mode = match options.get("mode").and_then(Value::as_str) {
        Some("search") => CollectionMode::Search,
        Some("timeline") => CollectionMode::Timeline,
        Some("bookmarks") => CollectionMode::Bookmarks,
        Some(value) => bail!("Invalid X connector mode '{value}'"),
        None => bail!("X connector option 'mode' is required"),
    };

    let query = options.get("query").and_then(Value::as_str).map(str::trim);
    match mode {
        CollectionMode::Search => {
            let query = query
                .filter(|value| !value.is_empty())
                .context_query_required()?;
            Ok(CollectOptions {
                mode,
                query: Some(query.to_owned()),
            })
        }
        CollectionMode::Timeline | CollectionMode::Bookmarks => {
            if options.contains_key("query") {
                bail!("X connector query is supported only in search mode")
            }
            Ok(CollectOptions { mode, query: None })
        }
    }
}

trait RequiredQuery<'a> {
    fn context_query_required(self) -> Result<&'a str>;
}

impl<'a> RequiredQuery<'a> for Option<&'a str> {
    fn context_query_required(self) -> Result<&'a str> {
        self.ok_or_else(|| anyhow::anyhow!("X connector search mode requires a non-empty query"))
    }
}
