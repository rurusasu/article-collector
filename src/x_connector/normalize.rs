use super::{PluginCandidate, MAX_TITLE_CHARS};
use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use url::Url;

pub fn normalize_response(response: &Value) -> Result<Vec<PluginCandidate>> {
    let data = response
        .get("data")
        .and_then(Value::as_array)
        .context("X MCP response must contain a data array")?;
    let users = user_lookup(response)?;
    let mut seen_ids = HashSet::new();
    let mut candidates = Vec::with_capacity(data.len());

    for post in data {
        let id = post
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .context("X MCP post is missing a non-empty id")?;
        let text = post
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .context("X MCP post is missing non-empty text")?;
        if !seen_ids.insert(id) {
            continue;
        }

        let user = post
            .get("author_id")
            .and_then(Value::as_str)
            .and_then(|author_id| users.get(author_id));
        let username = user
            .and_then(|user| user.get("username"))
            .and_then(Value::as_str)
            .filter(|username| !username.trim().is_empty());
        let url = canonical_url(username, id)?;
        let mut metadata = Map::new();
        metadata.insert("tweet_id".to_string(), Value::String(id.to_string()));
        if let Some(author) = user
            .and_then(|user| user.get("name"))
            .and_then(Value::as_str)
            .filter(|author| !author.trim().is_empty())
        {
            metadata.insert("author".to_string(), Value::String(author.to_string()));
        }
        if let Some(username) = username {
            metadata.insert("username".to_string(), Value::String(username.to_string()));
        }
        if let Some(created_at) = post.get("created_at").and_then(Value::as_str) {
            metadata.insert(
                "created_at".to_string(),
                Value::String(created_at.to_string()),
            );
        }
        if let Some(metrics) = post.get("public_metrics") {
            metadata.insert("public_metrics".to_string(), metrics.clone());
        }

        candidates.push(PluginCandidate {
            title: title_from_text(text, id),
            url,
            content: text.to_string(),
            metadata,
        });
    }

    Ok(candidates)
}

fn user_lookup(response: &Value) -> Result<HashMap<String, Value>> {
    let Some(includes) = response.get("includes") else {
        return Ok(HashMap::new());
    };
    let users = includes
        .get("users")
        .and_then(Value::as_array)
        .context("X MCP includes.users must be an array when included")?;
    Ok(users
        .iter()
        .filter_map(|user| {
            user.get("id")
                .and_then(Value::as_str)
                .map(|id| (id.to_string(), user.clone()))
        })
        .collect())
}

fn canonical_url(username: Option<&str>, id: &str) -> Result<Url> {
    let raw = match username {
        Some(username) => format!("https://x.com/{username}/status/{id}"),
        None => format!("https://x.com/i/web/status/{id}"),
    };
    Url::parse(&raw).context("Failed to construct canonical X post URL")
}

fn title_from_text(text: &str, id: &str) -> String {
    let title = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("X post");
    let mut chars = title.chars();
    let bounded: String = chars.by_ref().take(MAX_TITLE_CHARS).collect();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else if bounded.is_empty() {
        format!("X post {id}")
    } else {
        bounded
    }
}

pub fn validate_no_reserved_metadata(candidate: &PluginCandidate) -> Result<()> {
    for field in ["source", "site", "rank"] {
        if candidate.metadata.contains_key(field) {
            bail!("X connector candidate contains reserved metadata field '{field}'")
        }
    }
    Ok(())
}
