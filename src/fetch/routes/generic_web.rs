use anyhow::Result;
use regex::Regex;
use serde_json::{json, Value};

pub async fn fetch_items(url: &str) -> Result<Vec<Value>> {
    eprintln!("Routing: {url} -> generic web fetch");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let html = client.get(url).send().await?.text().await?;

    let title_re = Regex::new(r"(?is)<title>(.*?)</title>")?;
    let title = title_re
        .captures(&html)
        .and_then(|captures| captures.get(1))
        .map(|match_| html_escape::decode_html_entities(match_.as_str().trim()).to_string())
        .unwrap_or_else(|| "untitled".to_string());

    let body = strip_html(&html)?;
    let content = body.chars().take(50000).collect::<String>();

    Ok(vec![json!({
        "title": title,
        "content": content,
        "url": url
    })])
}

pub fn strip_html(html: &str) -> Result<String> {
    let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>")?;
    let body = script_re.replace_all(html, "");
    let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>")?;
    let body = style_re.replace_all(&body, "");
    let tag_re = Regex::new(r"<[^>]+>")?;
    let body = tag_re.replace_all(&body, " ");
    let ws_re = Regex::new(r"\s+")?;
    Ok(ws_re.replace_all(&body, " ").trim().to_string())
}
