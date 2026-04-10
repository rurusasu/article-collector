use anyhow::{bail, Result};
use regex::Regex;
use reqwest::Client;
use serde_json::Value;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

pub async fn get_metadata(vid: &str) -> (String, String) {
    let url = format!(
        "https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v={vid}&format=json"
    );
    let client = Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match resp {
        Ok(r) => match r.json::<Value>().await {
            Ok(data) => {
                let title = data
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("untitled")
                    .to_string();
                let author = data
                    .get("author_name")
                    .and_then(|a| a.as_str())
                    .unwrap_or("")
                    .to_string();
                (title, author)
            }
            Err(_) => ("untitled".to_string(), String::new()),
        },
        Err(_) => ("untitled".to_string(), String::new()),
    }
}

pub async fn get_transcript(vid: &str) -> Result<String> {
    let client = Client::new();
    let page_url = format!("https://www.youtube.com/watch?v={vid}");
    let page = client
        .get(&page_url)
        .header("User-Agent", USER_AGENT)
        .header("Accept-Language", "en,ja;q=0.9")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .text()
        .await?;

    // Extract caption tracks JSON
    let tracks_re = Regex::new(r#""captionTracks":\s*(\[.*?\])"#)?;
    let tracks_match = tracks_re.captures(&page);

    if let Some(caps) = tracks_match {
        let tracks: Vec<Value> = serde_json::from_str(&caps[1]).unwrap_or_default();
        if tracks.is_empty() {
            return try_innertube(&client, &page, vid).await;
        }

        // Prefer English track, fallback to first
        let track = tracks
            .iter()
            .find(|t| t.get("languageCode").and_then(|l| l.as_str()) == Some("en"))
            .unwrap_or(&tracks[0]);

        let base_url = track.get("baseUrl").and_then(|u| u.as_str()).unwrap_or("");

        if base_url.is_empty() {
            return try_innertube(&client, &page, vid).await;
        }

        // Try json3 format first, then srv3/XML
        let text_re = Regex::new(r"<text[^>]*>(.*?)</text>")?;
        for fmt in &["json3", "srv3", ""] {
            let fetch_url = if fmt.is_empty() {
                base_url.to_string()
            } else {
                format!("{base_url}&fmt={fmt}")
            };

            let resp = client
                .get(&fetch_url)
                .header("User-Agent", USER_AGENT)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            let data = match resp {
                Ok(r) => r.text().await.unwrap_or_default(),
                Err(_) => continue,
            };

            if data.is_empty() {
                continue;
            }

            if *fmt == "json3" {
                if let Ok(j) = serde_json::from_str::<Value>(&data) {
                    let texts = extract_json3_texts(&j);
                    if !texts.is_empty() {
                        return Ok(texts.join(" "));
                    }
                }
            } else {
                // XML format
                let texts = extract_xml_texts(&text_re, &data);
                if !texts.is_empty() {
                    return Ok(texts.join(" "));
                }
            }
        }
    }

    // Fallback to innertube
    try_innertube(&client, &page, vid).await
}

fn extract_xml_texts(text_re: &Regex, data: &str) -> Vec<String> {
    text_re
        .captures_iter(data)
        .filter_map(|c| {
            let t = html_escape::decode_html_entities(&c[1]).trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .collect()
}

fn extract_json3_texts(j: &Value) -> Vec<String> {
    let mut texts = Vec::new();
    if let Some(events) = j.get("events").and_then(|e| e.as_array()) {
        for event in events {
            if let Some(segs) = event.get("segs").and_then(|s| s.as_array()) {
                for seg in segs {
                    if let Some(t) = seg.get("utf8").and_then(|u| u.as_str()) {
                        let t = t.trim();
                        if !t.is_empty() && t != "\n" {
                            texts.push(t.to_string());
                        }
                    }
                }
            }
        }
    }
    texts
}

async fn try_innertube(client: &Client, page: &str, _vid: &str) -> Result<String> {
    let params_re = Regex::new(
        r#""params":"([^"]+)"[^}]*"targetId":"engagement-panel-searchable-transcript""#,
    )?;

    let params = match params_re.captures(page) {
        Some(caps) => caps[1].to_string(),
        None => bail!("no caption tracks found"),
    };

    let body = serde_json::json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20260101.00.00"
            }
        },
        "params": params
    });

    let resp: Value = client
        .post("https://www.youtube.com/youtubei/v1/get_transcript?prettyPrint=false")
        .header("User-Agent", USER_AGENT)
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;

    let mut texts = Vec::new();
    if let Some(actions) = resp.get("actions").and_then(|a| a.as_array()) {
        for action in actions {
            let cue_groups = action
                .pointer("/updateEngagementPanelAction/content/transcriptRenderer/body/transcriptBodyRenderer/cueGroups");
            if let Some(groups) = cue_groups.and_then(|g| g.as_array()) {
                for group in groups {
                    let cues = group.pointer("/transcriptCueGroupRenderer/cues");
                    if let Some(cues) = cues.and_then(|c| c.as_array()) {
                        for cue in cues {
                            if let Some(text) = cue
                                .pointer("/transcriptCueRenderer/cue/simpleText")
                                .and_then(|t| t.as_str())
                            {
                                let t = text.trim();
                                if !t.is_empty() {
                                    texts.push(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if texts.is_empty() {
        bail!("all methods failed");
    }

    Ok(texts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── extract_json3_texts ──

    #[test]
    fn json3_extracts_segments() {
        let j = json!({
            "events": [
                {"segs": [{"utf8": "Hello "}]},
                {"segs": [{"utf8": "World"}]}
            ]
        });
        assert_eq!(extract_json3_texts(&j), vec!["Hello", "World"]);
    }

    #[test]
    fn json3_skips_empty_segments() {
        let j = json!({
            "events": [
                {"segs": [{"utf8": ""}]},
                {"segs": [{"utf8": "Text"}]}
            ]
        });
        assert_eq!(extract_json3_texts(&j), vec!["Text"]);
    }

    #[test]
    fn json3_skips_newline_only_segments() {
        let j = json!({
            "events": [
                {"segs": [{"utf8": "\n"}]},
                {"segs": [{"utf8": "Content"}]}
            ]
        });
        assert_eq!(extract_json3_texts(&j), vec!["Content"]);
    }

    #[test]
    fn json3_handles_missing_events() {
        let j = json!({});
        assert!(extract_json3_texts(&j).is_empty());
    }

    #[test]
    fn json3_handles_missing_segs() {
        let j = json!({"events": [{"duration": 1000}]});
        assert!(extract_json3_texts(&j).is_empty());
    }

    #[test]
    fn json3_multiple_segs_per_event() {
        let j = json!({
            "events": [{
                "segs": [
                    {"utf8": "Part A"},
                    {"utf8": "Part B"}
                ]
            }]
        });
        assert_eq!(extract_json3_texts(&j), vec!["Part A", "Part B"]);
    }

    // ── extract_xml_texts ──

    #[test]
    fn xml_extracts_text_nodes() {
        let re = Regex::new(r"<text[^>]*>(.*?)</text>").unwrap();
        let data = r#"<text start="0" dur="5">Hello</text><text start="5" dur="3">World</text>"#;
        assert_eq!(extract_xml_texts(&re, data), vec!["Hello", "World"]);
    }

    #[test]
    fn xml_skips_empty_text() {
        let re = Regex::new(r"<text[^>]*>(.*?)</text>").unwrap();
        let data = r#"<text start="0" dur="5"></text><text start="5" dur="3">Content</text>"#;
        assert_eq!(extract_xml_texts(&re, data), vec!["Content"]);
    }

    #[test]
    fn xml_decodes_html_entities() {
        let re = Regex::new(r"<text[^>]*>(.*?)</text>").unwrap();
        let data = r#"<text start="0" dur="5">Hello &amp; World</text>"#;
        assert_eq!(extract_xml_texts(&re, data), vec!["Hello & World"]);
    }

    #[test]
    fn xml_handles_no_matches() {
        let re = Regex::new(r"<text[^>]*>(.*?)</text>").unwrap();
        assert!(extract_xml_texts(&re, "no xml here").is_empty());
    }

    #[test]
    fn xml_trims_whitespace() {
        let re = Regex::new(r"<text[^>]*>(.*?)</text>").unwrap();
        let data = r#"<text start="0" dur="5">  Trimmed  </text>"#;
        assert_eq!(extract_xml_texts(&re, data), vec!["Trimmed"]);
    }
}
