use anyhow::Result;
use serde_json::{json, Value};

pub async fn fetch_items(url: &str) -> Result<Vec<Value>> {
    let tweet_id = crate::fetch::extract_tweet_id(url)?;

    eprintln!("Routing: {url} -> X/Twitter syndication API (tweet_id={tweet_id})");

    let syndication_url = format!(
        "https://cdn.syndication.twimg.com/tweet-result?id={}&token=0",
        tweet_id
    );

    let client = reqwest::Client::new();
    let response = client.get(&syndication_url).send().await;

    let items = match response {
        Ok(response) if response.status().is_success() => {
            let data: Value = response.json().await.unwrap_or(Value::Null);
            if let Some(text) = data.get("text").and_then(|text| text.as_str()) {
                let author = data
                    .get("user")
                    .and_then(|user| user.get("name"))
                    .and_then(|name| name.as_str())
                    .unwrap_or("");
                let title = text.chars().take(80).collect::<String>();
                vec![json!({
                    "title": title,
                    "url": url,
                    "content": text,
                    "author": author,
                    "type": "x"
                })]
            } else {
                fallback_tweet(url, &tweet_id)
            }
        }
        _ => fallback_tweet(url, &tweet_id),
    };

    Ok(items)
}

fn fallback_tweet(url: &str, tweet_id: &str) -> Vec<Value> {
    eprintln!("  WARN: Could not fetch tweet content. Saving URL reference only.");
    vec![json!({
        "url": url,
        "tweet_id": tweet_id,
        "content": "(public content unavailable)",
        "type": "x"
    })]
}
