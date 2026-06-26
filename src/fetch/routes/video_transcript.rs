use anyhow::Result;
use serde_json::{json, Value};

use crate::youtube;

pub async fn fetch_items(url: &str) -> Result<Vec<Value>> {
    let video_id = crate::fetch::extract_youtube_vid(url)?;

    eprintln!("Routing: {url} -> YouTube oEmbed + transcript (vid={video_id})");

    let (title, author) = youtube::get_metadata(&video_id).await;
    let content = match youtube::get_transcript(&video_id).await {
        Ok(text) => text,
        Err(error) => format!("(transcript unavailable: {error})"),
    };

    Ok(vec![json!({
        "title": title,
        "author": author,
        "url": url,
        "content": content,
        "type": "youtube"
    })])
}
