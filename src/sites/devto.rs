use anyhow::Context;
use serde_json::{json, Value};

use super::types::{
    ArticleFetchFuture, DiscoveryEndpoint, FetchRoute, JsonRequest, SaveType, Site, UrlRule,
};

pub const DEVTO_ARTICLES_API: &str = "https://dev.to/api/articles?top=7";
pub const DEVTO_ARTICLE_API_BASE: &str = "https://dev.to/api/articles";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["dev.to/"])];

pub const SITE: Site = Site {
    name: "devto",
    aliases: &["dev.to"],
    supported_urls: &["https://dev.to/<author>/<slug>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::SiteArticleApi,
    save_type: SaveType::Web,
    save_rules: &[],
    discovery: Some(DiscoveryEndpoint::JsonApi {
        api_url: DEVTO_ARTICLES_API,
        request: JsonRequest::PaginatedPerPage,
    }),
    parse_discovery: None,
    fetch_article: Some(fetch_article),
};

pub fn fetch_article(url: &str) -> ArticleFetchFuture<'_> {
    fetch_article_from_api(url, DEVTO_ARTICLE_API_BASE)
}

pub fn fetch_article_from_api<'a>(url: &'a str, api_base: &'a str) -> ArticleFetchFuture<'a> {
    Box::pin(async move {
        let slug = crate::fetch::extract_devto_slug(url)?;
        eprintln!("Routing: {url} -> Dev.to API (slug={slug})");

        let api_url = format!("{}/{slug}", api_base.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .user_agent(concat!("article-collector/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let data: Value = client
            .get(&api_url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch Dev.to API response from {api_url}"))?
            .error_for_status()
            .with_context(|| format!("Dev.to API returned an error status for {api_url}"))?
            .json()
            .await
            .context("Failed to parse Dev.to API response")?;

        let content = data
            .get("body_markdown")
            .or_else(|| data.get("body_html"))
            .and_then(|content| content.as_str())
            .unwrap_or("");

        Ok(vec![json!({
            "title": data.get("title").and_then(|title| title.as_str()).unwrap_or(""),
            "url": data.get("url").and_then(|url| url.as_str()).unwrap_or(""),
            "content": content,
            "author": data.get("user").and_then(|user| user.get("name")).and_then(|name| name.as_str()).unwrap_or(""),
            "tags": devto_tags(&data),
            "published_at": data.get("readable_publish_date").and_then(|published| published.as_str()).unwrap_or(""),
            "reactions": data.get("public_reactions_count").and_then(|reactions| reactions.as_u64()).unwrap_or(0)
        })])
    })
}

fn devto_tags(data: &Value) -> Vec<String> {
    if let Some(tags) = data.get("tags").and_then(|tags| tags.as_array()) {
        return tags
            .iter()
            .filter_map(|tag| tag.as_str())
            .map(ToOwned::to_owned)
            .collect();
    }

    if let Some(tags) = data.get("tag_list").and_then(|tags| tags.as_array()) {
        return tags
            .iter()
            .filter_map(|tag| tag.as_str())
            .map(ToOwned::to_owned)
            .collect();
    }

    data.get("tag_list")
        .and_then(|tags| tags.as_str())
        .map(|tags| {
            tags.split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
