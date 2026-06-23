use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleArtifact {
    pub item: Value,
    pub json_path: PathBuf,
    pub translated_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleFailure {
    pub url: String,
    pub title: String,
    pub stage: String,
    pub error: String,
}

pub fn article_file_stem(
    rank: usize,
    source: &str,
    title: &str,
    used: &mut HashMap<String, usize>,
) -> String {
    let source = slugify(source).unwrap_or_else(|| "source".to_string());
    let title = slugify(title).unwrap_or_else(|| "untitled".to_string());
    let base = format!("{rank:03}-{source}-{title}");
    let count = used.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

pub fn format_article_content(item: &Value) -> String {
    let title = string_field(item, "title").unwrap_or("Untitled");
    let mut lines = vec![format!("## {title}"), String::new()];
    push_meta(&mut lines, "Source", string_field(item, "source"));
    push_meta(&mut lines, "Site", string_field(item, "site"));
    if let Some(rank) = item.get("rank").and_then(Value::as_u64) {
        lines.push(format!("Rank: {rank}"));
    }
    push_meta(&mut lines, "URL", string_field(item, "url"));
    push_meta(&mut lines, "Author", string_field(item, "author"));
    push_meta(&mut lines, "Published", string_field(item, "published_at"));
    lines.push(String::new());
    lines.push(
        string_field(item, "article_content")
            .unwrap_or("")
            .to_string(),
    );
    lines.join("\n")
}

pub fn write_article_json(articles_dir: &Path, stem: &str, item: &Value) -> Result<PathBuf> {
    fs::create_dir_all(articles_dir)?;
    let path = articles_dir.join(format!("{stem}.json"));
    fs::write(&path, serde_json::to_string_pretty(item)?)?;
    Ok(path)
}

pub fn write_failure_artifact(
    outdir: &Path,
    failures: &[ArticleFailure],
) -> Result<Option<PathBuf>> {
    if failures.is_empty() {
        return Ok(None);
    }
    fs::create_dir_all(outdir)?;
    let path = outdir.join("recommend-fetch-failures.json");
    let values = failures
        .iter()
        .map(|failure| {
            json!({
                "url": failure.url,
                "title": failure.title,
                "stage": failure.stage,
                "error": failure.error
            })
        })
        .collect::<Vec<_>>();
    fs::write(&path, serde_json::to_string_pretty(&values)?)?;
    Ok(Some(path))
}

pub fn write_translated_index(
    outdir: &Path,
    target: &str,
    artifacts: &[ArticleArtifact],
    has_failures: bool,
) -> Result<PathBuf> {
    fs::create_dir_all(outdir)?;
    let mut lines = vec![
        "# Recommended Articles".to_string(),
        String::new(),
        format!("Generated from `recommend {target} --fetch-articles`."),
        String::new(),
        "## Translated".to_string(),
        String::new(),
    ];

    for (index, artifact) in artifacts
        .iter()
        .filter(|artifact| artifact.translated_path.is_some())
        .enumerate()
    {
        let title = string_field(&artifact.item, "title").unwrap_or("Untitled");
        let translated_path = artifact.translated_path.as_ref().unwrap();
        let relative = translated_path
            .strip_prefix(outdir)
            .unwrap_or(translated_path)
            .to_string_lossy()
            .replace('\\', "/");
        lines.push(format!("{}. [{}]({})", index + 1, title, relative));
        if let Some(source) = string_field(&artifact.item, "source") {
            lines.push(format!("   - Source: {source}"));
        }
        if let Some(url) = string_field(&artifact.item, "url") {
            lines.push(format!("   - URL: {url}"));
        }
        lines.push(String::new());
    }

    if has_failures {
        lines.push("## Fetch Failures".to_string());
        lines.push(String::new());
        lines.push("See `recommend-fetch-failures.json`.".to_string());
        lines.push(String::new());
    }

    let path = outdir.join("translated.md");
    fs::write(&path, lines.join("\n"))?;
    Ok(path)
}

fn string_field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn push_meta(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        lines.push(format!("{label}: {value}"));
    }
}

fn slugify(value: &str) -> Option<String> {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
        if slug.len() >= 60 {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    (!slug.is_empty()).then_some(slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn builds_safe_article_file_stems() {
        let mut used = HashMap::new();
        assert_eq!(
            article_file_stem(1, "hackernews", "Hello, Rust!", &mut used),
            "001-hackernews-hello-rust"
        );
        assert_eq!(
            article_file_stem(2, "hackernews", "Hello, Rust!", &mut used),
            "002-hackernews-hello-rust"
        );
        assert_eq!(
            article_file_stem(3, "zenn", "!!!", &mut used),
            "003-zenn-untitled"
        );
    }

    #[test]
    fn formats_article_translation_content_as_h2_markdown() {
        let item = json!({
            "title": "Example Article",
            "source": "hackernews",
            "site": "hackernews",
            "rank": 1,
            "url": "https://example.com/article",
            "author": "alice",
            "published_at": "2026-06-23",
            "article_content": "Article body"
        });

        let content = format_article_content(&item);

        assert!(content.starts_with("## Example Article\n"));
        assert!(content.contains("Source: hackernews"));
        assert!(content.contains("Site: hackernews"));
        assert!(content.contains("Rank: 1"));
        assert!(content.contains("URL: https://example.com/article"));
        assert!(content.contains("Author: alice"));
        assert!(content.contains("Published: 2026-06-23"));
        assert!(content.ends_with("Article body"));
    }

    #[test]
    fn writes_translated_index_with_links() {
        let outdir = temp_outdir("translated-index");
        fs::create_dir_all(outdir.join("recommended_articles")).unwrap();
        let artifacts = vec![ArticleArtifact {
            item: json!({
                "title": "Example Article",
                "source": "hackernews",
                "url": "https://example.com/article"
            }),
            json_path: outdir.join("recommended_articles/001-hackernews-example-article.json"),
            translated_path: Some(
                outdir.join("recommended_articles/001-hackernews-example-article_translated.md"),
            ),
        }];

        write_translated_index(&outdir, "all", &artifacts, true).unwrap();

        let index = fs::read_to_string(outdir.join("translated.md")).unwrap();
        assert!(index.contains("# Recommended Articles"));
        assert!(index.contains(
            "[Example Article](recommended_articles/001-hackernews-example-article_translated.md)"
        ));
        assert!(index.contains("Source: hackernews"));
        assert!(index.contains("URL: https://example.com/article"));
        assert!(index.contains("recommend-fetch-failures.json"));
    }

    fn temp_outdir(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "article-collector-{name}-{}-{suffix}",
            std::process::id()
        ))
    }
}
