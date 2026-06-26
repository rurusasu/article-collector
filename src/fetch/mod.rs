use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::Value;
use std::fs;

pub mod article;
pub mod router;
pub mod routes;

use crate::paths;

pub use crate::sites::FetchRoute as Route;

pub fn classify_url(url: &str) -> Route {
    router::classify_url(url)
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
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string())
        .context("Could not extract tweet ID from URL")
}

pub fn extract_youtube_vid(url: &str) -> Result<String> {
    let re = Regex::new(r"(?:v=|youtu\.be/)([^&]+)")?;
    re.captures(url)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string())
        .context("Could not extract YouTube video ID")
}

pub fn extract_hn_id(url: &str) -> Result<String> {
    let re = Regex::new(r"id=(\d+)")?;
    re.captures(url)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string())
        .context("Could not extract HN item ID")
}

pub fn extract_devto_slug(url: &str) -> Result<String> {
    let re = Regex::new(r"https?://dev\.to/(.+?)/?$")?;
    re.captures(url)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string())
        .context("Could not extract Dev.to slug")
}

pub async fn fetch_url(url: &str) -> Result<()> {
    let result = fetch_url_items(url).await?;
    let outdir = paths::temp_dir();
    fs::create_dir_all(&outdir)?;
    let outfile = outdir.join("raw.json");
    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;

    eprintln!("Fetch complete: {}/", outdir.display());
    Ok(())
}

pub async fn fetch_url_items(url: &str) -> Result<Vec<Value>> {
    validate_url(url)?;
    if is_pdf_url(url) {
        bail!("PDF article fetching is not supported yet: {url}");
    }

    match classify_url(url) {
        Route::GenericWeb => routes::generic_web::fetch_items(url).await,
        Route::SocialStatus => routes::social_status::fetch_items(url).await,
        Route::VideoTranscript => routes::video_transcript::fetch_items(url).await,
        Route::SiteArticleApi => routes::site_article_api::fetch_items(url).await,
    }
}

pub fn is_pdf_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .is_some_and(|last| last.to_ascii_lowercase().ends_with(".pdf"))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn detects_pdf_urls_for_article_fetch_skip() {
        assert!(is_pdf_url("https://example.com/paper.pdf"));
        assert!(is_pdf_url("https://example.com/paper.PDF?download=1"));
        assert!(!is_pdf_url("https://example.com/article"));
    }

    #[tokio::test]
    async fn fetch_url_items_returns_generic_article_items() {
        let url = serve_html_article(
            "<html><head><title>Example</title></head><body><article>Hello article</article></body></html>",
        )
        .await;

        let items = fetch_url_items(&url).await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "Example");
        assert_eq!(items[0]["url"], url);
        assert!(items[0]["content"]
            .as_str()
            .unwrap()
            .contains("Hello article"));
    }

    #[tokio::test]
    async fn devto_site_adapter_uses_user_agent_and_parses_live_api_shape() {
        let api_base = serve_devto_api_article().await;

        let items = crate::sites::devto::fetch_article_from_api(
            "https://dev.to/alice/live-shape",
            &api_base,
        )
        .await
        .unwrap();

        assert_eq!(items[0]["title"], "Live Shape");
        assert_eq!(items[0]["url"], "https://dev.to/alice/live-shape");
        assert_eq!(items[0]["content"], "Body from markdown");
        assert_eq!(items[0]["author"], "Alice");
        assert_eq!(items[0]["tags"][0], "rust");
        assert_eq!(items[0]["tags"][1], "cli");
        assert_eq!(items[0]["published_at"], "Jun 24");
        assert_eq!(items[0]["reactions"], 7);
    }

    #[test]
    fn routes_hn_url() {
        assert_eq!(
            classify_url("https://news.ycombinator.com/item?id=123"),
            Route::SiteArticleApi
        );
    }

    #[test]
    fn routes_devto_url() {
        assert_eq!(
            classify_url("https://dev.to/author/slug"),
            Route::SiteArticleApi
        );
    }

    #[test]
    fn routes_youtube_watch_url() {
        assert_eq!(
            classify_url("https://www.youtube.com/watch?v=abc123"),
            Route::VideoTranscript
        );
    }

    #[test]
    fn routes_youtu_be_url() {
        assert_eq!(
            classify_url("https://youtu.be/abc123"),
            Route::VideoTranscript
        );
    }

    #[test]
    fn routes_x_com_url() {
        assert_eq!(
            classify_url("https://x.com/user/status/123456"),
            Route::SocialStatus
        );
    }

    #[test]
    fn routes_twitter_com_url() {
        assert_eq!(
            classify_url("https://twitter.com/user/status/123456"),
            Route::SocialStatus
        );
    }

    #[test]
    fn routes_unknown_url_to_generic() {
        assert_eq!(
            classify_url("https://example.com/article"),
            Route::GenericWeb
        );
    }

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

    #[test]
    fn strip_html_removes_script_tags() {
        let html = "<p>Hello</p><script>alert('xss')</script><p>World</p>";
        let result = routes::generic_web::strip_html(html).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn strip_html_removes_script_and_style() {
        let html = "<script>var x=1;</script><p>Text</p><style>.a{}</style>";
        let result = routes::generic_web::strip_html(html).unwrap();
        assert_eq!(result, "Text");
    }

    #[test]
    fn strip_html_preserves_text_content() {
        let html = "<h1>Title</h1><p>Paragraph with <strong>bold</strong> text.</p>";
        let result = routes::generic_web::strip_html(html).unwrap();
        assert_eq!(result, "Title Paragraph with bold text.");
    }

    async fn serve_devto_api_article() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request_buffer = [0_u8; 2048];
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let bytes_read = socket.read(&mut request_buffer).await.unwrap();
            let request = String::from_utf8_lossy(&request_buffer[..bytes_read]).to_string();
            let lower_request = request.to_ascii_lowercase();
            let has_expected_path = request.starts_with("GET /alice/live-shape ");
            let has_user_agent = lower_request
                .lines()
                .any(|line| line.starts_with("user-agent: article-collector/"));

            let body = if has_expected_path && has_user_agent {
                r#"{"title":"Live Shape","url":"https://dev.to/alice/live-shape","body_markdown":"Body from markdown","tag_list":"rust, cli","tags":["rust","cli"],"readable_publish_date":"Jun 24","public_reactions_count":7,"user":{"name":"Alice"}}"#
            } else {
                "missing user agent or wrong path"
            };
            let status = if has_expected_path && has_user_agent {
                "200 OK"
            } else {
                "403 Forbidden"
            };
            let content_type = if has_expected_path && has_user_agent {
                "application/json"
            } else {
                "text/plain"
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        format!("http://{address}")
    }

    async fn serve_html_article(body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request_buffer = [0_u8; 1024];
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let _ = socket.read(&mut request_buffer).await.unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        format!("http://{address}/article")
    }
}
