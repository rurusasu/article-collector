use anyhow::Context;
use serde_json::{json, Value};

use super::types::{
    ArticleFetchFuture, DiscoveryEndpoint, FetchRoute, JsonRequest, SaveType, Site, UrlRule,
};

pub const HN_TOPSTORIES_API: &str = "https://hacker-news.firebaseio.com/v0/topstories.json";
pub const HN_ITEM_API_TEMPLATE: &str = "https://hacker-news.firebaseio.com/v0/item/{id}.json";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["news.ycombinator.com/item"])];

pub const SITE: Site = Site {
    name: "hackernews",
    aliases: &["hn", "hacker-news"],
    supported_urls: &["https://news.ycombinator.com/item?id=<id>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::SiteArticleApi,
    save_type: SaveType::Web,
    save_rules: &[],
    discovery: Some(DiscoveryEndpoint::JsonApi {
        api_url: HN_TOPSTORIES_API,
        request: JsonRequest::FollowUpIds {
            item_url_template: HN_ITEM_API_TEMPLATE,
        },
    }),
    parse_discovery: None,
    fetch_article: Some(fetch_article),
};

pub fn fetch_article(url: &str) -> ArticleFetchFuture<'_> {
    Box::pin(async move {
        let id = crate::fetch::extract_hn_id(url)?;
        eprintln!("Routing: {url} -> HN Firebase API (id={id})");

        let api_url = HN_ITEM_API_TEMPLATE.replace("{id}", &id);
        let data: Value = reqwest::get(&api_url)
            .await?
            .json()
            .await
            .context("Failed to parse HN API response")?;

        let hn_url = data
            .get("url")
            .and_then(|url| url.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "https://news.ycombinator.com/item?id={}",
                    data.get("id").and_then(|id| id.as_u64()).unwrap_or(0)
                )
            });

        Ok(vec![json!({
            "title": data.get("title").and_then(|title| title.as_str()).unwrap_or(""),
            "url": hn_url,
            "author": data.get("by").and_then(|by| by.as_str()).unwrap_or(""),
            "score": data.get("score").and_then(|score| score.as_u64()).unwrap_or(0),
            "text": data.get("text").and_then(|text| text.as_str()).unwrap_or(""),
            "time": data.get("time").and_then(|time| time.as_u64()).unwrap_or(0),
            "type": data.get("type").and_then(|kind| kind.as_str()).unwrap_or(""),
            "descendants": data.get("descendants").and_then(|descendants| descendants.as_u64()).unwrap_or(0)
        })])
    })
}
