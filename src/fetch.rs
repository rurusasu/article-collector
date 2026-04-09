use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use crate::youtube;

const OUTDIR: &str = "/tmp/collect";

#[derive(Debug, PartialEq)]
pub enum Route {
    Twitter,
    YouTube,
    HackerNews,
    DevTo,
    Generic,
}

pub fn classify_url(url: &str) -> Route {
    if (url.contains("x.com/") && url.contains("/status/"))
        || (url.contains("twitter.com/") && url.contains("/status/"))
    {
        Route::Twitter
    } else if url.contains("youtube.com/watch") || url.contains("youtu.be/") {
        Route::YouTube
    } else if url.contains("news.ycombinator.com/item") {
        Route::HackerNews
    } else if url.contains("dev.to/") {
        Route::DevTo
    } else {
        Route::Generic
    }
}

pub fn validate_url(url: &str) -> Result<()> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("Invalid URL format: {url}");
    }
    Ok(())
}

pub fn extract_tweet_id(url: &str) -> Result<String> {
    let re = Regex::new(r"/status/(\d+)")?;
    re.captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("Could not extract tweet ID from URL")
}

pub fn extract_youtube_vid(url: &str) -> Result<String> {
    let re = Regex::new(r"(?:v=|youtu\.be/)([^&]+)")?;
    re.captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("Could not extract YouTube video ID")
}

pub fn extract_hn_id(url: &str) -> Result<String> {
    let re = Regex::new(r"id=(\d+)")?;
    re.captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("Could not extract HN item ID")
}

pub fn extract_devto_slug(url: &str) -> Result<String> {
    let re = Regex::new(r"https?://dev\.to/(.+?)/?$")?;
    re.captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("Could not extract Dev.to slug")
}

pub async fn fetch_url(url: &str) -> Result<()> {
    validate_url(url)?;

    fs::create_dir_all(OUTDIR)?;
    let outfile = Path::new(OUTDIR).join("raw.json");

    match classify_url(url) {
        Route::Twitter => fetch_twitter(url, &outfile).await,
        Route::YouTube => fetch_youtube(url, &outfile).await,
        Route::HackerNews => fetch_hackernews(url, &outfile).await,
        Route::DevTo => fetch_devto(url, &outfile).await,
        Route::Generic => fetch_generic(url, &outfile).await,
    }?;

    eprintln!("Fetch complete: {OUTDIR}/");
    Ok(())
}

async fn fetch_twitter(url: &str, outfile: &Path) -> Result<()> {
    let tweet_id = extract_tweet_id(url)?;

    eprintln!("Routing: {url} → X/Twitter syndication API (tweet_id={tweet_id})");

    let syndication_url = format!(
        "https://cdn.syndication.twimg.com/tweet-result?id={}&token=0",
        tweet_id
    );

    let client = reqwest::Client::new();
    let resp = client.get(&syndication_url).send().await;

    let result = match resp {
        Ok(r) if r.status().is_success() => {
            let data: Value = r.json().await.unwrap_or(Value::Null);
            if let Some(text) = data.get("text").and_then(|t| t.as_str()) {
                let author = data
                    .get("user")
                    .and_then(|u| u.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let title: String = text.chars().take(80).collect();
                json!([{
                    "title": title,
                    "url": url,
                    "content": text,
                    "author": author,
                    "type": "x"
                }])
            } else {
                fallback_tweet(url, &tweet_id)
            }
        }
        _ => fallback_tweet(url, &tweet_id),
    };

    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}

fn fallback_tweet(url: &str, tweet_id: &str) -> Value {
    eprintln!("  WARN: Could not fetch tweet content. Saving URL reference only.");
    json!([{
        "url": url,
        "tweet_id": tweet_id,
        "content": "(public content unavailable)",
        "type": "x"
    }])
}

async fn fetch_youtube(url: &str, outfile: &Path) -> Result<()> {
    let vid = extract_youtube_vid(url)?;

    eprintln!("Routing: {url} → YouTube oEmbed + transcript (vid={vid})");

    let (title, author) = youtube::get_metadata(&vid).await;
    let content = match youtube::get_transcript(&vid).await {
        Ok(text) => text,
        Err(e) => format!("(transcript unavailable: {e})"),
    };

    let result = json!([{
        "title": title,
        "author": author,
        "url": url,
        "content": content,
        "type": "youtube"
    }]);

    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}

async fn fetch_hackernews(url: &str, outfile: &Path) -> Result<()> {
    let id = extract_hn_id(url)?;

    eprintln!("Routing: {url} → HN Firebase API (id={id})");

    let api_url = format!("https://hacker-news.firebaseio.com/v0/item/{id}.json");
    let data: Value = reqwest::get(&api_url)
        .await?
        .json()
        .await
        .context("Failed to parse HN API response")?;

    let hn_url = data
        .get("url")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            format!(
                "https://news.ycombinator.com/item?id={}",
                data.get("id").and_then(|i| i.as_u64()).unwrap_or(0)
            )
        });

    let result = json!([{
        "title": data.get("title").and_then(|t| t.as_str()).unwrap_or(""),
        "url": hn_url,
        "author": data.get("by").and_then(|b| b.as_str()).unwrap_or(""),
        "score": data.get("score").and_then(|s| s.as_u64()).unwrap_or(0),
        "text": data.get("text").and_then(|t| t.as_str()).unwrap_or(""),
        "time": data.get("time").and_then(|t| t.as_u64()).unwrap_or(0),
        "type": data.get("type").and_then(|t| t.as_str()).unwrap_or(""),
        "descendants": data.get("descendants").and_then(|d| d.as_u64()).unwrap_or(0)
    }]);

    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}

async fn fetch_devto(url: &str, outfile: &Path) -> Result<()> {
    let slug = extract_devto_slug(url)?;

    eprintln!("Routing: {url} → Dev.to API (slug={slug})");

    let api_url = format!("https://dev.to/api/articles/{slug}");
    let data: Value = reqwest::get(&api_url)
        .await?
        .json()
        .await
        .context("Failed to parse Dev.to API response")?;

    let content = data
        .get("body_markdown")
        .or_else(|| data.get("body_html"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let tags: Vec<String> = data
        .get("tag_list")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let result = json!([{
        "title": data.get("title").and_then(|t| t.as_str()).unwrap_or(""),
        "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(""),
        "content": content,
        "author": data.get("user").and_then(|u| u.get("name")).and_then(|n| n.as_str()).unwrap_or(""),
        "tags": tags,
        "published_at": data.get("readable_publish_date").and_then(|p| p.as_str()).unwrap_or(""),
        "reactions": data.get("public_reactions_count").and_then(|r| r.as_u64()).unwrap_or(0)
    }]);

    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}

async fn fetch_generic(url: &str, outfile: &Path) -> Result<()> {
    eprintln!("Routing: {url} → generic web fetch");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let html = client.get(url).send().await?.text().await?;

    // Extract title
    let title_re = Regex::new(r"(?is)<title>(.*?)</title>")?;
    let title = title_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| html_escape::decode_html_entities(m.as_str().trim()).to_string())
        .unwrap_or_else(|| "untitled".to_string());

    // Remove script/style tags, then all HTML tags
    let script_re = Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>")?;
    let body = script_re.replace_all(&html, "");
    let tag_re = Regex::new(r"<[^>]+>")?;
    let body = tag_re.replace_all(&body, " ");
    let ws_re = Regex::new(r"\s+")?;
    let body = ws_re.replace_all(&body, " ").trim().to_string();

    // Truncate to 50000 chars
    let content: String = body.chars().take(50000).collect();

    let result = json!([{
        "title": title,
        "content": content,
        "url": url
    }]);

    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── URL validation ──

    #[test]
    fn rejects_url_without_http() {
        assert!(validate_url("not-a-url").is_err());
    }

    #[test]
    fn rejects_ftp_url() {
        assert!(validate_url("ftp://example.com/file").is_err());
    }

    #[test]
    fn accepts_https_url() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn accepts_http_url() {
        assert!(validate_url("http://example.com").is_ok());
    }

    // ── URL routing ──

    #[test]
    fn routes_hn_url() {
        assert_eq!(
            classify_url("https://news.ycombinator.com/item?id=123"),
            Route::HackerNews
        );
    }

    #[test]
    fn routes_devto_url() {
        assert_eq!(classify_url("https://dev.to/author/slug"), Route::DevTo);
    }

    #[test]
    fn routes_youtube_watch_url() {
        assert_eq!(
            classify_url("https://www.youtube.com/watch?v=abc123"),
            Route::YouTube
        );
    }

    #[test]
    fn routes_youtu_be_url() {
        assert_eq!(classify_url("https://youtu.be/abc123"), Route::YouTube);
    }

    #[test]
    fn routes_x_com_url() {
        assert_eq!(
            classify_url("https://x.com/user/status/123456"),
            Route::Twitter
        );
    }

    #[test]
    fn routes_twitter_com_url() {
        assert_eq!(
            classify_url("https://twitter.com/user/status/123456"),
            Route::Twitter
        );
    }

    #[test]
    fn routes_unknown_url_to_generic() {
        assert_eq!(classify_url("https://example.com/article"), Route::Generic);
    }

    // ── ID / slug extraction ──

    #[test]
    fn extracts_hn_item_id() {
        let id = extract_hn_id("https://news.ycombinator.com/item?id=42575").unwrap();
        assert_eq!(id, "42575");
    }

    #[test]
    fn extracts_devto_slug_from_url() {
        let slug = extract_devto_slug("https://dev.to/authorname/my-article-slug").unwrap();
        assert_eq!(slug, "authorname/my-article-slug");
    }

    #[test]
    fn extracts_youtube_vid_from_watch() {
        let vid = extract_youtube_vid("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(vid, "dQw4w9WgXcQ");
    }

    #[test]
    fn extracts_youtube_vid_from_youtu_be() {
        let vid = extract_youtube_vid("https://youtu.be/abc123XYZ").unwrap();
        assert_eq!(vid, "abc123XYZ");
    }

    #[test]
    fn extracts_youtube_vid_with_extra_params() {
        let vid = extract_youtube_vid("https://www.youtube.com/watch?v=xyz789&t=120").unwrap();
        assert_eq!(vid, "xyz789");
    }

    #[test]
    fn extracts_tweet_id_from_x() {
        let id = extract_tweet_id("https://x.com/user/status/123456789").unwrap();
        assert_eq!(id, "123456789");
    }

    #[test]
    fn extracts_tweet_id_from_twitter() {
        let id = extract_tweet_id("https://twitter.com/user/status/987654321").unwrap();
        assert_eq!(id, "987654321");
    }
}
