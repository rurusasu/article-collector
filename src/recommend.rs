use anyhow::{bail, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::config::{RecommendConfig, RecommendSourceConfig};
use crate::fetch;
use crate::paths;
use crate::sites::{self, RecommendSource, Site};

#[derive(Debug, PartialEq, Eq)]
struct LinkCandidate {
    title: String,
    url: String,
}

#[derive(Default)]
struct ZennFeedItem {
    title: String,
    url: String,
    description: String,
    published_at: String,
    author: String,
}

#[derive(Default)]
struct ArxivFeedEntry {
    id: String,
    title: String,
    url: String,
    summary: String,
    published_at: String,
    updated_at: String,
    authors: Vec<String>,
    categories: Vec<String>,
}

enum RecommendationTarget<'a> {
    AllSources,
    Source {
        site_name: &'static str,
        source: RecommendSource,
    },
    PageLinks {
        url: &'a str,
    },
}

const MAX_LIMIT: usize = 100;
const DEFAULT_LIMIT: usize = 30;
const ALL_TARGET: &str = "all";
const USER_AGENT: &str = concat!("article-collector/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
struct SourcePlan {
    site_name: &'static str,
    source: RecommendSource,
    limit: usize,
    query: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecommendationCollection {
    pub item_count: usize,
    pub source_count: usize,
    pub raw_path: PathBuf,
    pub translation_required: bool,
}

pub async fn collect_recommended(
    target: &str,
    limit: Option<usize>,
    query: Option<&str>,
    config: &RecommendConfig,
) -> Result<RecommendationCollection> {
    let recommendation_target = resolve_recommendation_target(target)?;
    let translation_required = recommendation_target.translation_required();
    let items = match recommendation_target {
        RecommendationTarget::AllSources => {
            reject_query_override(query)?;
            collect_all_sources(limit, config).await?
        }
        RecommendationTarget::Source { site_name, source } => {
            let plan = source_plan_for_parts(site_name, source, limit, query, config)?;
            collect_source(
                plan.site_name,
                plan.source,
                plan.limit,
                plan.query.as_deref(),
            )
            .await?
        }
        RecommendationTarget::PageLinks { url } => {
            reject_query_override(query)?;
            let limit = effective_limit(limit, config.limit, None)?;
            collect_page_links(url, "generic-web", None, limit).await?
        }
    };

    if items.is_empty() {
        bail!("No recommended articles found for {target}");
    }

    let outdir = paths::outdir();
    fs::create_dir_all(&outdir)?;
    let raw_path = paths::raw_json_path();
    fs::write(&raw_path, serde_json::to_string_pretty(&items)?)?;

    eprintln!(
        "Recommended articles collected: {} item(s) -> {}",
        items.len(),
        raw_path.display()
    );
    Ok(RecommendationCollection {
        item_count: items.len(),
        source_count: source_count_for_target(target, config)?,
        raw_path,
        translation_required,
    })
}

fn resolve_recommendation_target(target: &str) -> Result<RecommendationTarget<'_>> {
    if is_all_target(target) {
        if sites::recommendable_sites().is_empty() {
            bail!("No recommendation sources configured");
        }
        return Ok(RecommendationTarget::AllSources);
    }

    if let Some(site) = sites::site_by_name(target) {
        return site
            .recommend
            .map(|source| RecommendationTarget::Source {
                site_name: site.name,
                source,
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No recommendation source configured for site '{}'. Use one of: {}, {}",
                    site.name,
                    ALL_TARGET,
                    sites::recommendable_site_names().join(", ")
                )
            });
    }

    if fetch::validate_url(target).is_err() {
        bail!(
            "Unknown site or invalid URL: {target}. Use one of: {}, {}. Supported URL examples: {}",
            ALL_TARGET,
            sites::recommendable_site_names().join(", "),
            sites::supported_url_examples().join(", ")
        );
    }

    if let Some(source) = sites::recommend_source_for_url(target) {
        let site_name = sites::SITES
            .iter()
            .find(|site| site.recommend == Some(source))
            .map(|site| site.name)
            .unwrap_or("unknown");
        Ok(RecommendationTarget::Source { site_name, source })
    } else {
        Ok(RecommendationTarget::PageLinks { url: target })
    }
}

impl RecommendationTarget<'_> {
    fn translation_required(&self) -> bool {
        matches!(self, Self::AllSources)
    }
}

fn is_all_target(target: &str) -> bool {
    target.trim().eq_ignore_ascii_case(ALL_TARGET)
}

fn source_count_for_target(target: &str, config: &RecommendConfig) -> Result<usize> {
    Ok(match resolve_recommendation_target(target)? {
        RecommendationTarget::AllSources => source_plans_for_all(None, config)?.len(),
        RecommendationTarget::Source { .. } | RecommendationTarget::PageLinks { .. } => 1,
    })
}

fn validate_limit(limit: usize) -> Result<()> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        bail!("limit must be between 1 and {MAX_LIMIT}");
    }
    Ok(())
}

fn reject_query_override(query: Option<&str>) -> Result<()> {
    if query.is_some_and(|query| !query.trim().is_empty()) {
        bail!("--query is only supported for queryable recommendation sources such as arxiv");
    }
    Ok(())
}

async fn collect_hackernews_topstories(api_url: &str, limit: usize) -> Result<Vec<Value>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let top_ids: Vec<u64> = client
        .get(api_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("Failed to parse HN topstories response")?;

    if top_ids.is_empty() {
        bail!("Hacker News topstories returned no IDs");
    }

    let scan_limit = top_ids.len().min(limit.saturating_mul(10).max(20));
    let mut items = Vec::new();

    for id in top_ids.into_iter().take(scan_limit) {
        if items.len() >= limit {
            break;
        }

        let api_url = format!("https://hacker-news.firebaseio.com/v0/item/{id}.json");
        let response = client.get(&api_url).send().await;
        let data = match response {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(data) => data,
                    Err(err) => {
                        eprintln!("WARN: Failed to parse HN item {id}: {err}");
                        continue;
                    }
                },
                Err(err) => {
                    eprintln!("WARN: Failed to fetch HN item {id}: {err}");
                    continue;
                }
            },
            Err(err) => {
                eprintln!("WARN: Failed to fetch HN item {id}: {err}");
                continue;
            }
        };

        if let Some(item) = hackernews_item_to_recommendation(&data, items.len() + 1) {
            items.push(item);
        }
    }

    Ok(items)
}

fn hackernews_item_to_recommendation(data: &Value, rank: usize) -> Option<Value> {
    let url = data.get("url")?.as_str()?.to_string();
    if url.trim().is_empty() {
        return None;
    }

    let id = data.get("id").and_then(Value::as_u64).unwrap_or(0);
    let title = data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let author = data.get("by").and_then(Value::as_str).unwrap_or("");
    let score = data.get("score").and_then(Value::as_u64).unwrap_or(0);
    let comments = data.get("descendants").and_then(Value::as_u64).unwrap_or(0);
    let hn_url = format!("https://news.ycombinator.com/item?id={id}");
    let content = format!(
        "Title: {title}\nURL: {url}\nHacker News: {hn_url}\nScore: {score}\nComments: {comments}"
    );

    Some(json!({
        "rank": rank,
        "source": "hackernews",
        "id": id,
        "title": title,
        "url": url,
        "hn_url": hn_url,
        "author": author,
        "score": score,
        "comments": comments,
        "time": data.get("time").and_then(Value::as_u64).unwrap_or(0),
        "type": data.get("type").and_then(Value::as_str).unwrap_or("story"),
        "content": content
    }))
}

async fn collect_devto_articles(api_url: &str, limit: usize) -> Result<Vec<Value>> {
    let mut url = Url::parse(api_url).context("Invalid Dev.to articles API URL")?;
    url.query_pairs_mut()
        .append_pair("per_page", &limit.to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let articles: Vec<Value> = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("Failed to parse Dev.to articles response")?;

    Ok(articles
        .into_iter()
        .filter_map(|article| devto_article_to_recommendation(&article))
        .take(limit)
        .enumerate()
        .map(|(index, mut item)| {
            if let Some(object) = item.as_object_mut() {
                object.insert("rank".to_string(), json!(index + 1));
            }
            item
        })
        .collect())
}

fn devto_article_to_recommendation(data: &Value) -> Option<Value> {
    let url = data.get("url")?.as_str()?.to_string();
    if url.trim().is_empty() {
        return None;
    }

    let title = data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let author = data
        .get("user")
        .and_then(|user| user.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let description = data
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let tags = data
        .get("tag_list")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let content = format!(
        "Title: {title}\nURL: {url}\nAuthor: {author}\nTags: {tags}\nDescription: {description}"
    );

    Some(json!({
        "source": "devto",
        "title": title,
        "url": url,
        "author": author,
        "description": description,
        "tags": tags,
        "published_at": data.get("published_at").and_then(Value::as_str).unwrap_or(""),
        "reactions": data.get("public_reactions_count").and_then(Value::as_u64).unwrap_or(0),
        "comments": data.get("comments_count").and_then(Value::as_u64).unwrap_or(0),
        "content": content
    }))
}

async fn collect_zenn_feed(feed_url: &str, limit: usize) -> Result<Vec<Value>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let feed = client
        .get(feed_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    parse_zenn_feed(&feed, limit)
}

fn parse_zenn_feed(feed: &str, limit: usize) -> Result<Vec<Value>> {
    let mut reader = Reader::from_str(feed);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut items = Vec::new();
    let mut current_item: Option<ZennFeedItem> = None;
    let mut current_field: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(element)) => {
                let name = local_xml_name(element.name().as_ref());
                if name == "item" {
                    current_item = Some(ZennFeedItem::default());
                } else if current_item.is_some() {
                    current_field = Some(name);
                }
            }
            Ok(Event::Text(text)) => {
                if let (Some(item), Some(field)) = (current_item.as_mut(), current_field.as_deref())
                {
                    apply_zenn_text(item, field, &text.decode()?);
                }
            }
            Ok(Event::CData(text)) => {
                if let (Some(item), Some(field)) = (current_item.as_mut(), current_field.as_deref())
                {
                    apply_zenn_text(item, field, &text.decode()?);
                }
            }
            Ok(Event::End(element)) => {
                let name = local_xml_name(element.name().as_ref());
                if name == "item" {
                    if let Some(item) = current_item.take().and_then(zenn_item_to_recommendation) {
                        items.push(item);
                    }
                    if items.len() >= limit {
                        break;
                    }
                }
                current_field = None;
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                bail!(
                    "Failed to parse Zenn feed at byte {}: {err}",
                    reader.error_position()
                );
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(items)
}

fn apply_zenn_text(item: &mut ZennFeedItem, field: &str, text: &str) {
    let text = normalize_xml_text(text);
    match field {
        "title" => item.title = text,
        "link" => item.url = text,
        "description" => item.description = text,
        "pubDate" => item.published_at = text,
        "creator" => item.author = text,
        _ => {}
    }
}

fn zenn_item_to_recommendation(item: ZennFeedItem) -> Option<Value> {
    let url = normalize_url(item.url);
    if url.is_empty() {
        return None;
    }

    let title = if item.title.is_empty() {
        "Untitled".to_string()
    } else {
        item.title
    };
    let content = format!(
        "Title: {title}\nURL: {url}\nAuthor: {}\nPublished: {}\nDescription: {}",
        item.author, item.published_at, item.description
    );

    Some(json!({
        "source": "zenn",
        "title": title,
        "url": url,
        "author": item.author,
        "published_at": item.published_at,
        "description": item.description,
        "content": content
    }))
}

async fn collect_arxiv_search(api_url: &str, query: &str, limit: usize) -> Result<Vec<Value>> {
    let url = build_arxiv_search_url(api_url, query, limit)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let feed = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    parse_arxiv_feed(&feed, limit)
}

fn build_arxiv_search_url(api_url: &str, query: &str, limit: usize) -> Result<Url> {
    let mut url = Url::parse(api_url).context("Invalid arXiv API URL")?;
    url.query_pairs_mut()
        .append_pair("search_query", query)
        .append_pair("start", "0")
        .append_pair("max_results", &limit.to_string())
        .append_pair("sortBy", "submittedDate")
        .append_pair("sortOrder", "descending");
    Ok(url)
}

fn parse_arxiv_feed(feed: &str, limit: usize) -> Result<Vec<Value>> {
    let mut reader = Reader::from_str(feed);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut items = Vec::new();
    let mut current_entry: Option<ArxivFeedEntry> = None;
    let mut current_field: Option<String> = None;
    let mut in_author = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(element)) => {
                let name = local_xml_name(element.name().as_ref());
                if name == "entry" {
                    current_entry = Some(ArxivFeedEntry::default());
                } else if current_entry.is_some() {
                    handle_arxiv_empty_like_element(&mut current_entry, &element)?;
                    if name == "author" {
                        in_author = true;
                    }
                    current_field = arxiv_text_field(&name, in_author);
                }
            }
            Ok(Event::Empty(element)) if current_entry.is_some() => {
                handle_arxiv_empty_like_element(&mut current_entry, &element)?;
            }
            Ok(Event::Text(text)) => {
                if let (Some(entry), Some(field)) =
                    (current_entry.as_mut(), current_field.as_deref())
                {
                    apply_arxiv_text(entry, field, &text.decode()?);
                }
            }
            Ok(Event::CData(text)) => {
                if let (Some(entry), Some(field)) =
                    (current_entry.as_mut(), current_field.as_deref())
                {
                    apply_arxiv_text(entry, field, &text.decode()?);
                }
            }
            Ok(Event::End(element)) => {
                let name = local_xml_name(element.name().as_ref());
                if name == "entry" {
                    if let Some(item) = current_entry.take().and_then(arxiv_entry_to_recommendation)
                    {
                        items.push(item);
                    }
                    if items.len() >= limit {
                        break;
                    }
                } else if name == "author" {
                    in_author = false;
                }
                current_field = None;
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                bail!(
                    "Failed to parse arXiv feed at byte {}: {err}",
                    reader.error_position()
                );
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(items)
}

fn handle_arxiv_empty_like_element(
    current_entry: &mut Option<ArxivFeedEntry>,
    element: &BytesStart<'_>,
) -> Result<()> {
    let Some(entry) = current_entry.as_mut() else {
        return Ok(());
    };
    let name = local_xml_name(element.name().as_ref());

    match name.as_str() {
        "link" => {
            let href = xml_attr_value(element, b"href")?;
            let rel = xml_attr_value(element, b"rel")?;
            if rel.as_deref().unwrap_or("alternate") == "alternate" {
                if let Some(href) = href {
                    entry.url = normalize_arxiv_url(&href);
                }
            }
        }
        "category" => {
            if let Some(term) = xml_attr_value(element, b"term")? {
                entry.categories.push(term);
            }
        }
        _ => {}
    }

    Ok(())
}

fn arxiv_text_field(name: &str, in_author: bool) -> Option<String> {
    match name {
        "id" | "title" | "summary" | "published" | "updated" => Some(name.to_string()),
        "name" if in_author => Some("author".to_string()),
        _ => None,
    }
}

fn apply_arxiv_text(entry: &mut ArxivFeedEntry, field: &str, text: &str) {
    let text = normalize_xml_text(text);
    match field {
        "id" => entry.id = text,
        "title" => entry.title = text,
        "summary" => entry.summary = text,
        "published" => entry.published_at = text,
        "updated" => entry.updated_at = text,
        "author" => entry.authors.push(text),
        _ => {}
    }
}

fn arxiv_entry_to_recommendation(entry: ArxivFeedEntry) -> Option<Value> {
    let url = if entry.url.is_empty() {
        normalize_arxiv_url(&entry.id)
    } else {
        entry.url
    };
    if url.is_empty() {
        return None;
    }

    let title = if entry.title.is_empty() {
        "Untitled".to_string()
    } else {
        entry.title
    };
    let authors = entry.authors.join(", ");
    let categories = entry.categories.join(", ");
    let content = format!(
        "Title: {title}\nURL: {url}\nAuthors: {authors}\nCategories: {categories}\nPublished: {}\nUpdated: {}\nSummary: {}",
        entry.published_at, entry.updated_at, entry.summary
    );

    Some(json!({
        "source": "arxiv",
        "title": title,
        "url": url,
        "authors": authors,
        "categories": categories,
        "published_at": entry.published_at,
        "updated_at": entry.updated_at,
        "summary": entry.summary,
        "content": content
    }))
}

fn local_xml_name(name: &[u8]) -> String {
    let raw = String::from_utf8_lossy(name);
    raw.rsplit_once(':')
        .map(|(_, local)| local.to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn xml_attr_value(element: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for attr in element.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == key {
            return Ok(Some(
                String::from_utf8_lossy(attr.value.as_ref()).to_string(),
            ));
        }
    }
    Ok(None)
}

fn normalize_xml_text(text: &str) -> String {
    html_escape::decode_html_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_url(url: String) -> String {
    url.trim().to_string()
}

fn normalize_arxiv_url(url: &str) -> String {
    url.trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .strip_prefix("arxiv.org/")
        .map(|path| format!("https://arxiv.org/{path}"))
        .unwrap_or_else(|| url.trim().to_string())
}

async fn collect_all_sources(limit: Option<usize>, config: &RecommendConfig) -> Result<Vec<Value>> {
    let source_plans = source_plans_for_all(limit, config)?;
    let mut items = Vec::new();

    for plan in source_plans {
        let mut site_items = collect_source(
            plan.site_name,
            plan.source,
            plan.limit,
            plan.query.as_deref(),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to collect recommended articles for site '{}'",
                plan.site_name
            )
        })?;
        if site_items.is_empty() {
            bail!(
                "No recommended articles found for site '{}'",
                plan.site_name
            );
        }

        items.append(&mut site_items);
    }

    Ok(items)
}

fn source_plans_for_all(
    cli_limit: Option<usize>,
    config: &RecommendConfig,
) -> Result<Vec<SourcePlan>> {
    let mut plans = Vec::new();
    for site in configured_recommendable_sites(config)? {
        if source_config_for(site.name, config).and_then(|source| source.enabled) == Some(false) {
            continue;
        }
        plans.push(source_plan_for_site(site, cli_limit, None, config)?);
    }
    if plans.is_empty() {
        bail!("No enabled recommendation sources configured");
    }
    Ok(plans)
}

fn configured_recommendable_sites(config: &RecommendConfig) -> Result<Vec<&'static Site>> {
    let Some(source_names) = config.sources.as_ref().filter(|names| !names.is_empty()) else {
        return Ok(sites::recommendable_sites());
    };

    source_names
        .iter()
        .map(|name| {
            let site = sites::site_by_name(name)
                .with_context(|| format!("Unknown recommendation source in config: {name}"))?;
            site.recommend
                .map(|_| site)
                .with_context(|| format!("No recommendation source configured for site '{name}'"))
        })
        .collect()
}

fn source_plan_for_site(
    site: &'static Site,
    cli_limit: Option<usize>,
    cli_query: Option<&str>,
    config: &RecommendConfig,
) -> Result<SourcePlan> {
    let source = site
        .recommend
        .context("recommendable site must have a recommend source")?;
    source_plan_for_parts(site.name, source, cli_limit, cli_query, config)
}

fn source_plan_for_parts(
    site_name: &'static str,
    source: RecommendSource,
    cli_limit: Option<usize>,
    cli_query: Option<&str>,
    config: &RecommendConfig,
) -> Result<SourcePlan> {
    let source_config = source_config_for(site_name, config);
    let source_limit = source_config.and_then(|source| source.limit);
    let query = effective_query(cli_query, source_config);
    let limit = effective_limit(cli_limit, config.limit, source_limit)?;

    Ok(SourcePlan {
        site_name,
        source,
        limit,
        query,
    })
}

fn source_config_for<'a>(
    site_name: &str,
    config: &'a RecommendConfig,
) -> Option<&'a RecommendSourceConfig> {
    config.source.get(site_name)
}

fn effective_limit(
    cli_limit: Option<usize>,
    config_limit: Option<usize>,
    source_limit: Option<usize>,
) -> Result<usize> {
    let limit = cli_limit
        .or(source_limit)
        .or(config_limit)
        .unwrap_or(DEFAULT_LIMIT);
    validate_limit(limit)?;
    Ok(limit)
}

fn effective_query(
    cli_query: Option<&str>,
    source_config: Option<&RecommendSourceConfig>,
) -> Option<String> {
    cli_query
        .filter(|query| !query.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            source_config
                .and_then(|source| source.query.as_deref())
                .filter(|query| !query.trim().is_empty())
                .map(ToOwned::to_owned)
        })
}

async fn collect_source(
    site_name: &'static str,
    source: RecommendSource,
    limit: usize,
    query: Option<&str>,
) -> Result<Vec<Value>> {
    let mut items = match source {
        RecommendSource::HackerNewsTopStories { api_url } => {
            reject_query_override(query)?;
            collect_hackernews_topstories(api_url, limit).await?
        }
        RecommendSource::DevToArticles { api_url } => {
            reject_query_override(query)?;
            collect_devto_articles(api_url, limit).await?
        }
        RecommendSource::ZennFeed { feed_url } => {
            reject_query_override(query)?;
            collect_zenn_feed(feed_url, limit).await?
        }
        RecommendSource::ArxivSearch {
            api_url,
            default_query,
        } => {
            collect_arxiv_search(
                api_url,
                query_override_or_default(query, default_query),
                limit,
            )
            .await?
        }
    };

    for item in &mut items {
        if let Some(object) = item.as_object_mut() {
            object.insert("site".to_string(), json!(site_name));
        }
    }

    Ok(items)
}

fn query_override_or_default<'a>(query: Option<&'a str>, default_query: &'a str) -> &'a str {
    query
        .filter(|query| !query.trim().is_empty())
        .unwrap_or(default_query)
}

async fn collect_page_links(
    url: &str,
    source_name: &str,
    site_name: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>> {
    let base = Url::parse(url).context("Invalid URL")?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(USER_AGENT)
        .build()?;
    let html = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let links = extract_page_links(&base, &html, limit);
    Ok(links
        .into_iter()
        .enumerate()
        .map(|(index, link)| {
            let rank = index + 1;
            let content = format!(
                "Title: {}\nURL: {}\nSource page: {}",
                link.title, link.url, url
            );
            json!({
                "rank": rank,
                "source": source_name,
                "title": link.title,
                "url": link.url,
                "source_url": url,
                "content": content
            })
        })
        .map(|mut item| {
            if let (Some(site_name), Some(object)) = (site_name, item.as_object_mut()) {
                object.insert("site".to_string(), json!(site_name));
            }
            item
        })
        .collect())
}

fn extract_page_links(base: &Url, html: &str, limit: usize) -> Vec<LinkCandidate> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").expect("valid selector");
    let mut seen = HashSet::new();
    let mut base_without_fragment = base.clone();
    base_without_fragment.set_fragment(None);
    let base_url = base_without_fragment.to_string();
    let mut links = Vec::new();

    for element in document.select(&selector) {
        if links.len() >= limit {
            break;
        }

        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Ok(mut link_url) = base.join(href) else {
            continue;
        };
        if link_url.scheme() != "http" && link_url.scheme() != "https" {
            continue;
        }
        link_url.set_fragment(None);

        let url = link_url.to_string();
        if url == base_url || !seen.insert(url.clone()) {
            continue;
        }

        let title = normalize_link_text(element.text().collect::<Vec<_>>().join(" "));
        links.push(LinkCandidate {
            title: if title.is_empty() { url.clone() } else { title },
            url,
        });
    }

    links
}

fn normalize_link_text(text: String) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RecommendConfig, RecommendSourceConfig};
    use std::collections::BTreeMap;

    /// 検証: Hacker News の site 名は topstories 収集として扱う
    /// 理由: 長い HN item URL を毎回入力せずに recommend を実行できるようにする
    /// リスク: site 名入力が使えずワンコマンド性が落ちる
    #[test]
    fn resolves_hackernews_site_name_as_topstories() {
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::HackerNewsTopStories { .. },
        } = resolve_recommendation_target("hackernews").unwrap()
        else {
            panic!("hackernews should resolve to HN topstories");
        };
        assert_eq!(site_name, "hackernews");
    }

    /// 検証: HN URL も topstories 収集として扱う
    /// 理由: 既存の URL 入力フローとの互換性を保つ
    /// リスク: HN URL が generic link 抽出に落ち、上位記事を収集できない
    #[test]
    fn resolves_hackernews_url_as_topstories() {
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::HackerNewsTopStories { .. },
        } = resolve_recommendation_target("https://news.ycombinator.com/item?id=123").unwrap()
        else {
            panic!("HN URL should resolve to HN topstories");
        };
        assert_eq!(site_name, "hackernews");
    }

    /// 検証: Dev.to の site 名は Dev.to API 収集へ解決する
    /// 理由: site 名だけで Dev.to の上位記事取得を開始できるようにする
    /// リスク: Dev.to が汎用リンク抽出に落ち、記事以外のリンクが混ざる
    #[test]
    fn resolves_devto_site_name_to_articles_api() {
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::DevToArticles { api_url },
        } = resolve_recommendation_target("devto").unwrap()
        else {
            panic!("devto should resolve to Dev.to articles");
        };
        assert_eq!(site_name, "devto");
        assert_eq!(api_url, "https://dev.to/api/articles?top=7");
    }

    /// 検証: Zenn の site 名はトレンド RSS 収集へ解決する
    /// 理由: `recommend all` で Zenn の技術トレンドも取得したい
    /// リスク: Zenn が generic link 抽出に落ち、記事推薦として正規化されない
    #[test]
    fn resolves_zenn_site_name_to_feed() {
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::ZennFeed { feed_url },
        } = resolve_recommendation_target("zenn").unwrap()
        else {
            panic!("zenn should resolve to Zenn feed");
        };
        assert_eq!(site_name, "zenn");
        assert_eq!(feed_url, "https://zenn.dev/feed");
    }

    /// 検証: arXiv の site 名は既定 query の arXiv API 収集へ解決する
    /// 理由: AI/ML/CV/NLP 系の新着論文を `recommend all` に含めたい
    /// リスク: 論文 source が登録されず、既存の fetch/save 対応だけに留まる
    #[test]
    fn resolves_arxiv_site_name_to_default_search() {
        let RecommendationTarget::Source {
            site_name,
            source:
                RecommendSource::ArxivSearch {
                    api_url,
                    default_query,
                },
        } = resolve_recommendation_target("arxiv").unwrap()
        else {
            panic!("arxiv should resolve to arXiv search");
        };
        assert_eq!(site_name, "arxiv");
        assert_eq!(api_url, "https://export.arxiv.org/api/query");
        assert!(default_query.contains("cat:cs.CV"));
        assert!(default_query.contains("cat:cs.LG"));
        assert!(default_query.contains("stat.ML"));
    }

    /// 検証: all は registry 上の全 recommend source 収集として扱う
    /// 理由: 個別 site 名を列挙せずに一括収集と翻訳を開始できるようにする
    /// リスク: all が未知 site として拒否され、バッチ用途で使えない
    #[test]
    fn resolves_all_keyword_to_all_sources() {
        let target = resolve_recommendation_target("ALL").unwrap();
        assert!(matches!(target, RecommendationTarget::AllSources));
        assert!(target.translation_required());
    }

    /// 検証: HN 以外の http(s) URL は generic page link 抽出として扱う
    /// 理由: まずはページ内リンクをワンコマンドで収集する fallback を提供する
    /// リスク: 未対応サイトで何も収集できない
    #[test]
    fn resolves_other_urls_as_generic_links() {
        let RecommendationTarget::PageLinks { url } =
            resolve_recommendation_target("https://example.com/article").unwrap()
        else {
            panic!("generic URL should resolve to page links");
        };
        assert_eq!(url, "https://example.com/article");
    }

    /// 検証: recommend source 未設定の site 名は明示的に拒否する
    /// 理由: site 名だけで無根拠な collection URL を推測しない
    /// リスク: YouTube 等で意図しないページからリンクを収集する
    #[test]
    fn rejects_site_names_without_recommend_source() {
        assert!(resolve_recommendation_target("youtube").is_err());
    }

    /// 検証: 未知の site 名は URL ではなく入力エラーとして扱う
    /// 理由: typo を generic URL として扱わない
    /// リスク: 誤入力が分かりにくいネットワークエラーになる
    #[test]
    fn rejects_unknown_site_names() {
        assert!(resolve_recommendation_target("unknown-site").is_err());
    }

    /// 検証: all の source 数は recommend 可能な site 数と一致する
    /// 理由: `--limit` は各 source ごとの上限として適用する
    /// リスク: 最初の site だけで上限を使い切り、後続 site から取得されない
    #[test]
    fn counts_all_sources_from_site_registry() {
        let config = RecommendConfig::default();
        assert_eq!(
            source_count_for_target("all", &config).unwrap(),
            sites::recommendable_sites().len()
        );
        assert_eq!(source_count_for_target("hackernews", &config).unwrap(), 1);
    }

    /// 検証: all は source 別 config で arXiv query と source limit を上書きする
    /// 理由: #news 用に arXiv の取得カテゴリだけを設定ファイルから調整したい
    /// リスク: all 実行時に registry 既定 query しか使えず、LLM/RAG/agent 寄りの論文収集に寄せられない
    #[test]
    fn all_source_plans_apply_arxiv_config_query_and_source_limit() {
        let config = RecommendConfig {
            limit: Some(30),
            sources: Some(vec!["arxiv".to_string()]),
            source: BTreeMap::from([(
                "arxiv".to_string(),
                RecommendSourceConfig {
                    limit: Some(10),
                    query: Some("cat:cs.IR OR cat:cs.SE".to_string()),
                    ..Default::default()
                },
            )]),
        };

        let plans = source_plans_for_all(None, &config).unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].site_name, "arxiv");
        assert_eq!(plans[0].limit, 10);
        assert_eq!(plans[0].query.as_deref(), Some("cat:cs.IR OR cat:cs.SE"));
    }

    /// 検証: all は config の source 順序を使い、enabled=false の source を除外する
    /// 理由: cron 用の all 実行で収集対象を設定ファイル側から安定して制御したい
    /// リスク: 不要 source を止められず、翻訳対象が増えたり外部 API failure に巻き込まれる
    #[test]
    fn all_source_plans_use_config_source_order_and_skip_disabled_sources() {
        let config = RecommendConfig {
            sources: Some(vec![
                "zenn".to_string(),
                "hackernews".to_string(),
                "devto".to_string(),
            ]),
            source: BTreeMap::from([(
                "devto".to_string(),
                RecommendSourceConfig {
                    enabled: Some(false),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };

        let plans = source_plans_for_all(None, &config).unwrap();
        let names = plans.iter().map(|plan| plan.site_name).collect::<Vec<_>>();

        assert_eq!(names, vec!["zenn", "hackernews"]);
    }

    /// 検証: arXiv 単体実行では CLI 指定が config より優先される
    /// 理由: 一時的な手動収集では config を編集せず query/limit を上書きしたい
    /// リスク: CLI で明示した query が config に隠れて、意図と違うカテゴリを取得する
    #[test]
    fn direct_arxiv_plan_prefers_cli_query_and_limit_over_config() {
        let config = RecommendConfig {
            limit: Some(30),
            source: BTreeMap::from([(
                "arxiv".to_string(),
                RecommendSourceConfig {
                    limit: Some(10),
                    query: Some("cat:cs.IR".to_string()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let site = sites::site_by_name("arxiv").unwrap();

        let plan = source_plan_for_site(site, Some(5), Some("cat:cs.SE"), &config).unwrap();

        assert_eq!(plan.site_name, "arxiv");
        assert_eq!(plan.limit, 5);
        assert_eq!(plan.query.as_deref(), Some("cat:cs.SE"));
    }

    /// 検証: limit は 1 以上 100 以下だけ許可する
    /// 理由: 誤入力で大量の外部リクエストを発生させない
    /// リスク: limit=0 や巨大値で空出力または過剰通信になる
    #[test]
    fn validates_limit_range() {
        assert!(validate_limit(1).is_ok());
        assert!(validate_limit(30).is_ok());
        assert!(validate_limit(100).is_ok());
        assert!(validate_limit(0).is_err());
        assert!(validate_limit(101).is_err());
    }

    /// 検証: HN item からリンク付き story を推薦項目へ変換する
    /// 理由: topstories API の item JSON を raw.json の配列要素に正規化する
    /// リスク: URL 付き story が欠落し、推薦一覧が空になる
    #[test]
    fn converts_hackernews_item_to_recommendation() {
        let item = json!({
            "id": 42,
            "title": "Example story",
            "url": "https://example.com/story",
            "by": "alice",
            "score": 123,
            "descendants": 45,
            "time": 1000,
            "type": "story"
        });

        let recommendation = hackernews_item_to_recommendation(&item, 1).unwrap();
        assert_eq!(recommendation["rank"], 1);
        assert_eq!(recommendation["source"], "hackernews");
        assert_eq!(recommendation["title"], "Example story");
        assert_eq!(recommendation["url"], "https://example.com/story");
        assert_eq!(
            recommendation["hn_url"],
            "https://news.ycombinator.com/item?id=42"
        );
    }

    /// 検証: Dev.to API item から記事推薦項目へ変換する
    /// 理由: `recommend devto` と `recommend all` で Dev.to の記事 API レスポンスを raw.json に正規化する
    /// リスク: Dev.to から記事 URL を取得しても翻訳対象の content が空になる
    #[test]
    fn converts_devto_article_to_recommendation() {
        let item = json!({
            "title": "Example Dev.to story",
            "url": "https://dev.to/example/story",
            "description": "Short description",
            "tag_list": ["rust", "cli"],
            "user": {"name": "alice"},
            "published_at": "2026-06-14T00:00:00Z",
            "public_reactions_count": 10,
            "comments_count": 2
        });

        let recommendation = devto_article_to_recommendation(&item).unwrap();
        assert_eq!(recommendation["source"], "devto");
        assert_eq!(recommendation["title"], "Example Dev.to story");
        assert_eq!(recommendation["url"], "https://dev.to/example/story");
        assert_eq!(recommendation["author"], "alice");
        assert_eq!(recommendation["tags"], "rust, cli");
        assert_eq!(
            recommendation["content"],
            "Title: Example Dev.to story\nURL: https://dev.to/example/story\nAuthor: alice\nTags: rust, cli\nDescription: Short description"
        );
    }

    /// 検証: Zenn RSS item を推薦項目へ正規化する
    /// 理由: RSS の title/link/description/pubDate を raw.json に安定して渡す
    /// リスク: XML 構造差分で Zenn 記事の URL または本文が欠落する
    #[test]
    fn parses_zenn_rss_items() {
        let rss = r#"
            <rss><channel>
                <item>
                    <title>Example Zenn article</title>
                    <link>https://zenn.dev/example/articles/abc</link>
                    <description>Article summary</description>
                    <pubDate>Sun, 14 Jun 2026 00:00:00 GMT</pubDate>
                    <dc:creator>alice</dc:creator>
                </item>
            </channel></rss>
        "#;

        let items = parse_zenn_feed(rss, 1).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["source"], "zenn");
        assert_eq!(items[0]["title"], "Example Zenn article");
        assert_eq!(items[0]["url"], "https://zenn.dev/example/articles/abc");
        assert_eq!(items[0]["author"], "alice");
    }

    /// 検証: arXiv Atom entry を推薦項目へ正規化する
    /// 理由: arXiv API の Atom XML から論文 title/link/summary/category を取得する
    /// リスク: arXiv API が JSON ではないため、XML parser 不備で結果が空になる
    #[test]
    fn parses_arxiv_atom_entries() {
        let atom = r#"
            <feed xmlns="http://www.w3.org/2005/Atom">
                <entry>
                    <id>http://arxiv.org/abs/2606.00001v1</id>
                    <updated>2026-06-14T00:00:00Z</updated>
                    <published>2026-06-14T00:00:00Z</published>
                    <title>Example vision paper</title>
                    <summary>Paper abstract</summary>
                    <author><name>Alice Example</name></author>
                    <author><name>Bob Example</name></author>
                    <link href="http://arxiv.org/abs/2606.00001v1" rel="alternate" type="text/html"/>
                    <category term="cs.CV"/>
                    <category term="cs.LG"/>
                </entry>
            </feed>
        "#;

        let items = parse_arxiv_feed(atom, 1).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["source"], "arxiv");
        assert_eq!(items[0]["title"], "Example vision paper");
        assert_eq!(items[0]["url"], "https://arxiv.org/abs/2606.00001v1");
        assert_eq!(items[0]["authors"], "Alice Example, Bob Example");
        assert_eq!(items[0]["categories"], "cs.CV, cs.LG");
    }

    /// 検証: arXiv API URL は指定 query と limit を反映する
    /// 理由: `recommend arxiv --query ...` で関心カテゴリを上書きしたい
    /// リスク: ユーザー指定 query が無視され、既定カテゴリしか取得できない
    #[test]
    fn builds_arxiv_search_url_with_custom_query() {
        let url = build_arxiv_search_url(
            "https://export.arxiv.org/api/query",
            "cat:cs.CV OR cat:cs.LG",
            5,
        )
        .unwrap();
        assert_eq!(url.host_str(), Some("export.arxiv.org"));
        assert!(url.as_str().contains("search_query=cat%3Acs.CV"));
        assert!(url.as_str().contains("max_results=5"));
        assert!(url.as_str().contains("sortBy=submittedDate"));
        assert!(url.as_str().contains("sortOrder=descending"));
    }

    /// 検証: 空の query override は arXiv 既定 query に戻す
    /// 理由: `--query ""` で空検索を投げず、通常の関心カテゴリ収集を維持する
    /// リスク: shell 変数展開ミスなどで空 query が渡ると arXiv 収集が失敗する
    #[test]
    fn blank_arxiv_query_uses_default_query() {
        assert_eq!(
            query_override_or_default(Some("   "), "cat:cs.CV"),
            "cat:cs.CV"
        );
        assert_eq!(
            query_override_or_default(Some("cat:cs.LG"), "cat:cs.CV"),
            "cat:cs.LG"
        );
    }

    /// 検証: URL がない Dev.to item は推薦項目から除外する
    /// 理由: 翻訳後に参照できない記事を raw.json に入れない
    /// リスク: 空 URL の候補が保存され、後続の確認や保存処理が壊れる
    #[test]
    fn skips_devto_articles_without_url() {
        let item = json!({"title": "No URL"});
        assert!(devto_article_to_recommendation(&item).is_none());
    }

    /// 検証: URL がない HN item はページ推薦から除外する
    /// 理由: Ask HN 等は推薦先ページを持たない
    /// リスク: 内部 discussion だけが上位ページとして混ざる
    #[test]
    fn skips_hackernews_items_without_url() {
        let item = json!({"id": 42, "title": "Ask HN"});
        assert!(hackernews_item_to_recommendation(&item, 1).is_none());
    }

    /// 検証: generic HTML から http(s) リンクを順序どおり抽出する
    /// 理由: 未対応サイトでもページ内の推薦リンク候補を raw.json にできる
    /// リスク: 相対 URL や重複リンクが壊れたまま出力される
    #[test]
    fn extracts_generic_page_links() {
        let base = Url::parse("https://example.com/articles/root#section").unwrap();
        let html = r#"
            <a href="/first"> First article </a>
            <a href="https://example.org/second#comments">Second article</a>
            <a href="/first">Duplicate first article</a>
            <a href="mailto:test@example.com">Mail</a>
            <a href="https://example.com/articles/root#other">Self</a>
        "#;

        let links = extract_page_links(&base, html, 10);
        assert_eq!(
            links,
            vec![
                LinkCandidate {
                    title: "First article".to_string(),
                    url: "https://example.com/first".to_string()
                },
                LinkCandidate {
                    title: "Second article".to_string(),
                    url: "https://example.org/second".to_string()
                }
            ]
        );
    }

    /// 検証: generic link 抽出は limit で停止する
    /// 理由: 指定件数以上の推薦候補を処理しない
    /// リスク: 大きなページで不要なリンクを大量に出力する
    #[test]
    fn generic_page_links_respect_limit() {
        let base = Url::parse("https://example.com/").unwrap();
        let html = r#"
            <a href="/a">A</a>
            <a href="/b">B</a>
            <a href="/c">C</a>
        "#;

        let links = extract_page_links(&base, html, 2);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].url, "https://example.com/a");
        assert_eq!(links[1].url, "https://example.com/b");
    }

    /// 検証: registry に登録された全 recommend source から実際に記事を取得できる
    /// 理由: `recommend all` は全 site の外部 API / ページ構造に依存するため、単体変換だけでは壊れた取得経路を検出できない
    /// リスク: ある site の source が壊れても all 実行まで気づけない
    #[tokio::test]
    async fn collects_recommendations_from_every_registered_site() {
        let config = RecommendConfig::default();
        let items = collect_all_sources(Some(1), &config).await.unwrap();
        let collected_sites = items
            .iter()
            .filter_map(|item| item.get("site").and_then(Value::as_str))
            .collect::<HashSet<_>>();

        for site in sites::recommendable_sites() {
            assert!(
                collected_sites.contains(site.name),
                "missing recommendations from {}",
                site.name
            );
        }

        assert!(items.iter().all(|item| item
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| url.starts_with("https://") || url.starts_with("http://"))));
    }
}
