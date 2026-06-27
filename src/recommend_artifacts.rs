use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const SUMMARY_EXCERPT_MAX_CHARS: usize = 180;

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
        format!("Generated from `recommend {target}` with `[recommend].fetch_articles = true`."),
        String::new(),
        "## Articles".to_string(),
        String::new(),
    ];

    for (index, artifact) in artifacts.iter().enumerate() {
        let title = string_field(&artifact.item, "title").unwrap_or("Untitled");
        lines.push(format!("{}. {title}", index + 1));
        push_item_metadata(&mut lines, &artifact.item);
        if let Some(summary) = summary_excerpt(&artifact.item, artifact.translated_path.as_deref())
        {
            lines.push(format!("   Summary: {summary}"));
        }
        if let Some(translated_path) = artifact.translated_path.as_ref() {
            let relative = relative_artifact_path(outdir, translated_path);
            lines.push(format!("   Translation: {relative}"));
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

fn push_item_metadata(lines: &mut Vec<String>, item: &Value) {
    push_value(lines, "URL", string_or_number_field(item, "url"));
    push_value(lines, "Hacker News", string_or_number_field(item, "hn_url"));
    push_value(lines, "Score", string_or_number_field(item, "score"));
    push_value(lines, "Comments", string_or_number_field(item, "comments"));
    push_value(lines, "Source", string_or_number_field(item, "source"));
    push_value(lines, "Site", string_or_number_field(item, "site"));
    push_value(lines, "Severity", string_or_number_field(item, "severity"));
    push_value(lines, "CVE", string_or_number_field(item, "cve_id"));
    push_value(lines, "GHSA", string_or_number_field(item, "ghsa_id"));
    push_value(lines, "CVSS", string_or_number_field(item, "cvss_score"));
    push_value(lines, "Stars", string_or_number_field(item, "stars"));
    push_value(lines, "Language", string_or_number_field(item, "language"));
    push_value(lines, "Author", string_or_number_field(item, "author"));
    push_value(
        lines,
        "Published",
        string_or_number_field(item, "published_at"),
    );
    push_value(lines, "Updated", string_or_number_field(item, "updated_at"));
}

fn push_value(lines: &mut Vec<String>, label: &str, value: Option<String>) {
    if let Some(value) = value {
        lines.push(format!("   {label}: {value}"));
    }
}

fn summary_excerpt(item: &Value, translated_path: Option<&Path>) -> Option<String> {
    translated_path
        .and_then(translation_excerpt)
        .or_else(|| metadata_excerpt(item))
}

fn translation_excerpt(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| markdown_excerpt(&content))
}

fn metadata_excerpt(item: &Value) -> Option<String> {
    ["summary", "description", "article_content", "content"]
        .into_iter()
        .filter_map(|key| string_field(item, key))
        .filter_map(markdown_excerpt)
        .next()
}

fn markdown_excerpt(content: &str) -> Option<String> {
    let mut paragraph = Vec::new();
    let mut in_code_block = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block || should_skip_excerpt_line(line) {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        paragraph.push(line);
    }

    let text = paragraph.join(" ");
    truncate_excerpt(&text)
}

fn should_skip_excerpt_line(line: &str) -> bool {
    if line.is_empty()
        || line == "---"
        || line.starts_with('#')
        || line.starts_with('|')
        || line.starts_with("![")
    {
        return true;
    }

    let Some((label, _)) = line.split_once(':') else {
        return false;
    };
    matches!(
        label.trim().to_ascii_lowercase().as_str(),
        "title"
            | "url"
            | "source"
            | "site"
            | "rank"
            | "author"
            | "authors"
            | "published"
            | "published_at"
            | "updated"
            | "updated_at"
            | "hacker news"
            | "score"
            | "comments"
            | "severity"
            | "cve"
            | "ghsa"
            | "cvss"
            | "cvss score"
            | "stars"
            | "language"
            | "tags"
            | "categories"
    )
}

fn truncate_excerpt(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }

    let mut truncated = normalized
        .chars()
        .take(SUMMARY_EXCERPT_MAX_CHARS)
        .collect::<String>();
    if normalized.chars().count() > SUMMARY_EXCERPT_MAX_CHARS {
        truncated = truncated.trim_end().to_string();
        truncated.push_str("...");
    }
    Some(truncated)
}

fn relative_artifact_path(outdir: &Path, path: &Path) -> String {
    path.strip_prefix(outdir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn string_field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn string_or_number_field(item: &Value, key: &str) -> Option<String> {
    match item.get(key)? {
        Value::String(value) => (!value.trim().is_empty()).then(|| value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
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
    fn writes_translated_index_with_translation_references() {
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
        assert!(index
            .contains("Generated from `recommend all` with `[recommend].fetch_articles = true`."));
        assert!(index.contains("1. Example Article"));
        assert!(index.contains(
            "Translation: recommended_articles/001-hackernews-example-article_translated.md"
        ));
        assert!(index.contains("Source: hackernews"));
        assert!(index.contains("URL: https://example.com/article"));
        assert!(index.contains("recommend-fetch-failures.json"));
    }

    #[test]
    fn writes_translated_index_with_slack_ready_metadata_and_summary_excerpt() {
        let outdir = temp_outdir("translated-index-summary");
        let translated_path =
            outdir.join("recommended_articles/001-hackernews-example_translated.md");
        fs::create_dir_all(translated_path.parent().unwrap()).unwrap();
        fs::write(
            &translated_path,
            "## Example Article\n\nSource: hackernews\n\nこれは推薦記事の翻訳済み本文です。Slack に貼るための短い抜粋として使います。\n\n続きの本文。",
        )
        .unwrap();
        let artifacts = vec![ArticleArtifact {
            item: json!({
                "title": "Example Article",
                "source": "hackernews",
                "url": "https://example.com/article",
                "hn_url": "https://news.ycombinator.com/item?id=123",
                "score": 43,
                "comments": 7,
                "description": "Fallback description"
            }),
            json_path: outdir.join("recommended_articles/001-hackernews-example.json"),
            translated_path: Some(translated_path),
        }];

        write_translated_index(&outdir, "all", &artifacts, false).unwrap();

        let index = fs::read_to_string(outdir.join("translated.md")).unwrap();
        assert!(index.contains("1. Example Article"));
        assert!(index.contains("   URL: https://example.com/article"));
        assert!(index.contains("   Hacker News: https://news.ycombinator.com/item?id=123"));
        assert!(index.contains("   Score: 43"));
        assert!(index.contains("   Comments: 7"));
        assert!(index.contains(
            "   Summary: これは推薦記事の翻訳済み本文です。Slack に貼るための短い抜粋として使います。"
        ));
        assert!(index
            .contains("   Translation: recommended_articles/001-hackernews-example_translated.md"));
    }

    #[test]
    fn writes_translated_index_summary_from_metadata_when_translation_excerpt_is_unavailable() {
        let outdir = temp_outdir("translated-index-summary-fallback");
        let artifacts = vec![ArticleArtifact {
            item: json!({
                "title": "Advisory Example",
                "source": "github-advisory",
                "url": "https://github.com/advisories/GHSA-abcd",
                "summary": "Patch the affected dependency before deploying the next release.",
                "severity": "high",
                "cve_id": "CVE-2026-0001",
                "ghsa_id": "GHSA-abcd"
            }),
            json_path: outdir.join("recommended_articles/001-github-advisory-example.json"),
            translated_path: None,
        }];

        write_translated_index(&outdir, "github-advisory", &artifacts, false).unwrap();

        let index = fs::read_to_string(outdir.join("translated.md")).unwrap();
        assert!(index.contains("1. Advisory Example"));
        assert!(index.contains("   URL: https://github.com/advisories/GHSA-abcd"));
        assert!(index.contains("   Severity: high"));
        assert!(index.contains("   CVE: CVE-2026-0001"));
        assert!(index.contains("   GHSA: GHSA-abcd"));
        assert!(index.contains(
            "   Summary: Patch the affected dependency before deploying the next release."
        ));
        assert!(!index.contains("Translation:"));
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
