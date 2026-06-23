# Recommend Fetch Articles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `article-collector recommend <target> --fetch-articles` so recommendation candidates can be fetched as full articles, translated per article, and written as local run artifacts.

**Architecture:** Keep recommendation source collection in `recommend.rs`, reuse existing article fetching from `fetch.rs`, reuse ACP translation from `translate.rs`, and add a focused `recommend_artifacts.rs` module for local outdir artifacts. `--fetch-articles` works for every target, writes only fetched article artifacts, records SQLite seen items only after article translation succeeds, skips PDFs for now, and never writes to the target repo or creates PRs.

**Tech Stack:** Rust stable, clap, tokio, reqwest, serde_json, scraper/regex existing fetch stack, SQLite history through existing `recommend_history.rs`, ACP translation through existing `translate.rs`.

---

## File Structure

- Modify: `src/main.rs`
  - Add `fetch_articles: bool` to the `Recommend` CLI variant.
  - Pass the flag into `recommend::collect_recommended`.
  - Do not call the old single `translate(raw.json)` path after `recommend --fetch-articles`.
- Modify: `src/fetch.rs`
  - Extract existing file-writing fetch code into `fetch_url_items(url) -> Result<Vec<Value>>`.
  - Keep `fetch_url(url)` as a wrapper that writes `raw.json`.
  - Skip obvious PDF URLs in the article-fetch path with a clear error.
- Modify: `src/translate.rs`
  - Add a reusable `translate_content(content) -> Result<TranslateContentOutcome>` API.
  - Keep existing `translate(input)` behavior for current commands.
- Modify: `src/recommend.rs`
  - Add `fetch_articles` orchestration after SQLite dedupe.
  - In `--fetch-articles` mode, only record seen items that completed article translation.
  - Keep the non-`--fetch-articles` path behavior unchanged.
- Create: `src/recommend_artifacts.rs`
  - Own `recommended_articles/`.
  - Own per-article JSON writes.
  - Own safe file stems: `001-source-title-slug`.
  - Own `recommend-fetch-failures.json`.
  - Own `translated.md` index for translated per-article Markdown files.
- Modify: `src/paths.rs`
  - Add helper paths for `recommended_articles/` and `recommend-fetch-failures.json`.
- Modify: `README.md`
  - Document `--fetch-articles`, all-target support, per-article artifacts, failure behavior, SQLite seen behavior, PDF coming soon, and `ACP_AGENT` behavior.

---

## Behavior Contract

- `--fetch-articles` is opt-in CLI-only.
- It works for `all`, site names, and URL targets.
- PDFs are not fetched in this release. They are recorded in `recommend-fetch-failures.json` with stage `unsupported_pdf`; README marks PDF extraction as coming soon.
- Fetch success writes `recommended_articles/*.json`.
- Translation success writes `recommended_articles/*_translated.md`.
- `translated.md` is an index, not a combined article body.
- `ACP_AGENT` unset:
  - Per-article JSON files are written.
  - Per-article translation files are not written.
  - `translated.md` is not written.
  - SQLite seen records are not inserted.
- `ACP_AGENT` set:
  - Translated articles appear in `translated.md`.
  - SQLite seen records are inserted only for translated articles.
  - Translation failure preserves the per-article JSON and records a failure entry.
  - If zero articles translate successfully, return non-zero.
- Target repo / PR behavior is out of scope for `recommend --fetch-articles`.

---

### Task 1: CLI Flag And Path Helpers

**Files:**
- Modify: `src/main.rs`
- Modify: `src/paths.rs`
- Modify: `tests/cli.rs`

- [ ] **Step 1: Write failing CLI help test**

Add to `tests/cli.rs`:

```rust
#[test]
fn recommend_help_lists_fetch_articles_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .args(["recommend", "--help"])
        .output()
        .expect("run article-collector recommend --help");

    assert!(
        output.status.success(),
        "expected recommend --help to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("help output should be valid UTF-8");
    assert!(
        stdout.contains("--fetch-articles"),
        "recommend help should list --fetch-articles:\n{stdout}"
    );
}
```

- [ ] **Step 2: Write failing path helper tests**

Add to `src/paths.rs` tests:

```rust
#[test]
fn recommend_article_paths_are_under_outdir() {
    let outdir = outdir();
    assert_eq!(recommended_articles_dir(), outdir.join("recommended_articles"));
    assert_eq!(
        recommend_fetch_failures_path(),
        outdir.join("recommend-fetch-failures.json")
    );
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo test --locked --test cli recommend_help_lists_fetch_articles_flag
cargo test --locked paths::tests::recommend_article_paths_are_under_outdir
```

Expected: FAIL because `--fetch-articles`, `recommended_articles_dir`, and `recommend_fetch_failures_path` do not exist.

- [ ] **Step 4: Add CLI flag and path helpers**

In `src/main.rs`, add to `Commands::Recommend`:

```rust
/// 推薦 URL の記事本文も取得して記事別 artifact を作成
#[arg(long)]
fetch_articles: bool,
```

Bind it as `_fetch_articles` in the `Recommend` match arm for now so this task compiles without changing recommend behavior. Task 5 passes it into `recommend::collect_recommended`.

In `src/paths.rs`, add:

```rust
pub fn recommended_articles_dir() -> PathBuf {
    outdir().join("recommended_articles")
}

pub fn recommend_fetch_failures_path() -> PathBuf {
    outdir().join("recommend-fetch-failures.json")
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --locked --test cli recommend_help_lists_fetch_articles_flag
cargo test --locked paths::tests::recommend_article_paths_are_under_outdir
```

Expected: PASS. The flag exists in CLI help, but behavior is still unchanged until Task 5.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/paths.rs tests/cli.rs
git commit -m "feat: add recommend fetch articles flag"
```

---

### Task 2: Reusable Fetch API

**Files:**
- Modify: `src/fetch.rs`

- [ ] **Step 1: Write failing fetch API tests**

Add these tests to `src/fetch.rs` tests:

```rust
#[test]
fn detects_pdf_urls_for_article_fetch_skip() {
    assert!(is_pdf_url("https://example.com/paper.pdf"));
    assert!(is_pdf_url("https://example.com/paper.PDF?download=1"));
    assert!(!is_pdf_url("https://example.com/article"));
}

#[tokio::test]
async fn fetch_url_items_returns_generic_article_items() {
    let url = serve_html_article("<html><head><title>Example</title></head><body><article>Hello article</article></body></html>").await;

    let items = fetch_url_items(&url).await.unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Example");
    assert_eq!(items[0]["url"], url);
    assert!(items[0]["content"].as_str().unwrap().contains("Hello article"));
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --locked fetch::tests::detects_pdf_urls_for_article_fetch_skip
cargo test --locked fetch::tests::fetch_url_items_returns_generic_article_items
```

Expected: FAIL because `is_pdf_url` and `fetch_url_items` do not exist.

- [ ] **Step 3: Extract fetch functions**

Change `fetch_url` to:

```rust
pub async fn fetch_url(url: &str) -> Result<()> {
    let result = fetch_url_items(url).await?;
    let outdir = paths::outdir();
    fs::create_dir_all(&outdir)?;
    let outfile = outdir.join("raw.json");
    fs::write(outfile, serde_json::to_string_pretty(&result)?)?;
    eprintln!("Fetch complete: {}/", outdir.display());
    Ok(())
}
```

Add:

```rust
pub async fn fetch_url_items(url: &str) -> Result<Vec<Value>> {
    validate_url(url)?;
    if is_pdf_url(url) {
        bail!("PDF article fetching is not supported yet: {url}");
    }

    let result = match classify_url(url) {
        Route::Twitter => fetch_twitter_items(url).await,
        Route::YouTube => fetch_youtube_items(url).await,
        Route::HackerNews => fetch_hackernews_items(url).await,
        Route::DevTo => fetch_devto_items(url).await,
        Route::Generic => fetch_generic_items(url).await,
    }?;

    Ok(result
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![result]))
}

pub fn is_pdf_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.path_segments().and_then(|mut segments| segments.next_back().map(str::to_string)))
        .is_some_and(|last| last.to_ascii_lowercase().ends_with(".pdf"))
}
```

Refactor existing route functions from `fetch_*(&Path)` into item-returning functions:

```rust
async fn fetch_generic_items(url: &str) -> Result<Value> {
    eprintln!("Routing: {url} → generic web fetch");
    // existing generic implementation, but return `result` instead of writing it
}
```

Keep file-writing wrappers only if useful inside tests; do not duplicate network logic.

- [ ] **Step 4: Run fetch tests**

Run:

```bash
cargo test --locked fetch::tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/fetch.rs
git commit -m "refactor: expose reusable fetch items"
```

---

### Task 3: Reusable Translation API

**Files:**
- Modify: `src/translate.rs`

- [ ] **Step 1: Write failing translation API tests**

Add to `src/translate.rs` tests:

```rust
#[test]
fn translate_content_skips_without_agent_value() {
    assert_eq!(
        translate_content_outcome_for_agent(None, "hello").unwrap(),
        TranslateContentOutcome::Skipped
    );
    assert_eq!(
        translate_content_outcome_for_agent(Some("  "), "hello").unwrap(),
        TranslateContentOutcome::Skipped
    );
}

#[test]
fn translate_content_rejects_empty_content() {
    assert!(translate_content_outcome_for_agent(Some("codex"), "  ").is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --locked translate::tests::translate_content_skips_without_agent_value
cargo test --locked translate::tests::translate_content_rejects_empty_content
```

Expected: FAIL because `TranslateContentOutcome` and `translate_content_outcome_for_agent` do not exist.

- [ ] **Step 3: Add reusable translation API**

In `src/translate.rs`, add:

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum TranslateContentOutcome {
    Translated(String),
    Skipped,
}

pub async fn translate_content(content: &str) -> Result<TranslateContentOutcome> {
    if content.trim().is_empty() {
        bail!("No content extracted for translation");
    }

    let Some(agent) = acp_agent_from_env()? else {
        eprintln!("Error: ACP_AGENT is not set. Translation skipped.");
        return Ok(TranslateContentOutcome::Skipped);
    };

    let lang = std::env::var("TRANSLATE_LANG").unwrap_or_else(|_| "ja".to_string());
    let translated = translate_text(agent, &lang, content).await?;
    Ok(TranslateContentOutcome::Translated(translated))
}

fn translate_content_outcome_for_agent(
    agent: Option<&str>,
    content: &str,
) -> Result<TranslateContentOutcome> {
    let agent = acp_agent_from_value(agent)?;
    if content.trim().is_empty() {
        bail!("No content extracted for translation");
    }
    if agent.is_none() {
        return Ok(TranslateContentOutcome::Skipped);
    }
    Ok(TranslateContentOutcome::Translated(String::new()))
}
```

This keeps the existing prompt behavior in `translate_text` and gives `recommend` a way to translate one prepared Markdown string without writing the top-level `translated.md`.

- [ ] **Step 4: Reuse API inside existing `translate`**

Change the main content translation block in `translate(input)` to use the new lower-level pieces without changing external behavior:

```rust
let translated = translate_text(agent, &lang, &content).await?;
```

can remain as-is if the new per-content API shares `translate_text`. Do not change `translate(input)` output path or `TranslateOutcome`.

- [ ] **Step 5: Run translation tests**

Run:

```bash
cargo test --locked translate::tests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/translate.rs
git commit -m "refactor: expose reusable content translation"
```

---

### Task 4: Recommend Artifact Module

**Files:**
- Create: `src/recommend_artifacts.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write artifact module tests**

Create `src/recommend_artifacts.rs` with:

```rust
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

#[cfg(test)]
mod tests {
    use super::*;

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
            translated_path: Some(outdir.join("recommended_articles/001-hackernews-example-article_translated.md")),
        }];

        write_translated_index(&outdir, "all", &artifacts, true).unwrap();

        let index = fs::read_to_string(outdir.join("translated.md")).unwrap();
        assert!(index.contains("# Recommended Articles"));
        assert!(index.contains("[Example Article](recommended_articles/001-hackernews-example-article_translated.md)"));
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --locked recommend_artifacts::tests
```

Expected: FAIL to compile because `mod recommend_artifacts` and helper functions do not exist.

- [ ] **Step 3: Register module and implement helpers**

Add to `src/main.rs`:

```rust
mod recommend_artifacts;
```

Add implementations to `src/recommend_artifacts.rs`:

```rust
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

pub fn write_article_json(
    articles_dir: &Path,
    stem: &str,
    item: &Value,
) -> Result<PathBuf> {
    fs::create_dir_all(articles_dir)?;
    let path = articles_dir.join(format!("{stem}.json"));
    fs::write(&path, serde_json::to_string_pretty(item)?)?;
    Ok(path)
}

pub fn write_failure_artifact(outdir: &Path, failures: &[ArticleFailure]) -> Result<Option<PathBuf>> {
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
    item.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty())
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
```

- [ ] **Step 4: Run artifact tests**

Run:

```bash
cargo test --locked recommend_artifacts::tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/recommend_artifacts.rs
git commit -m "feat: add recommend article artifacts"
```

---

### Task 5: Recommend Fetch Articles Orchestration

**Files:**
- Modify: `src/recommend.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing orchestration tests**

Add pure helper tests to `src/recommend.rs` tests:

```rust
#[test]
fn fetch_articles_seen_items_include_only_translated_artifacts() {
    let translated = vec![
        json!({"url": "https://example.com/translated", "title": "Translated"}),
    ];
    let fetched_not_translated = vec![
        json!({"url": "https://example.com/fetched", "title": "Fetched"}),
    ];

    let seen = seen_items_for_fetch_articles(&translated, &fetched_not_translated);

    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["url"], "https://example.com/translated");
}

#[test]
fn fetch_articles_requires_translated_items_when_agent_is_configured() {
    let error = ensure_fetch_articles_success("all", true, 0).unwrap_err();

    assert_eq!(
        error.to_string(),
        "No recommended articles translated for all"
    );
}

#[test]
fn fetch_articles_allows_json_only_when_translation_is_skipped() {
    assert!(ensure_fetch_articles_success("all", false, 0).is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --locked recommend::tests::fetch_articles_seen_items_include_only_translated_artifacts
cargo test --locked recommend::tests::fetch_articles_requires_translated_items_when_agent_is_configured
cargo test --locked recommend::tests::fetch_articles_allows_json_only_when_translation_is_skipped
```

Expected: FAIL because helper functions do not exist.

- [ ] **Step 3: Add helper functions**

Add in `src/recommend.rs`:

```rust
fn seen_items_for_fetch_articles(translated_items: &[Value], _fetched_items: &[Value]) -> Vec<Value> {
    translated_items.to_vec()
}

fn ensure_fetch_articles_success(
    target: &str,
    translation_was_attempted: bool,
    translated_count: usize,
) -> Result<()> {
    if translation_was_attempted && translated_count == 0 {
        bail!("No recommended articles translated for {target}");
    }
    Ok(())
}
```

- [ ] **Step 4: Implement `collect_recommended` flag branch**

Change signature:

```rust
pub async fn collect_recommended(
    target: &str,
    limit: Option<usize>,
    query: Option<&str>,
    config: &RecommendConfig,
    fetch_articles: bool,
) -> Result<RecommendationCollection>
```

After SQLite dedupe and `ensure_new_recommendations`, branch:

```rust
if fetch_articles {
    return collect_recommended_articles(target, items, history, dedup_outcome, config).await;
}
```

Implement the new async function in `src/recommend.rs`:

```rust
async fn collect_recommended_articles(
    target: &str,
    items: Vec<Value>,
    mut history: RecommendationHistory,
    dedup_outcome: crate::recommend_history::DedupOutcome,
    _config: &RecommendConfig,
) -> Result<RecommendationCollection> {
    let outdir = paths::outdir();
    let articles_dir = paths::recommended_articles_dir();
    fs::create_dir_all(&articles_dir)?;

    let mut used = std::collections::HashMap::new();
    let mut artifacts = Vec::new();
    let mut failures = Vec::new();
    let mut translated_items = Vec::new();
    let mut fetched_items = Vec::new();
    let mut translation_attempted = false;

    for (index, item) in items.into_iter().enumerate() {
        let url = item.get("url").and_then(Value::as_str).unwrap_or("").to_string();
        let title = item.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_string();
        let source = item.get("source").and_then(Value::as_str).unwrap_or("recommend");
        let stem = recommend_artifacts::article_file_stem(index + 1, source, &title, &mut used);

        let fetched = match fetch::fetch_url_items(&url).await {
            Ok(values) => values.into_iter().next().unwrap_or(Value::Null),
            Err(error) => {
                failures.push(recommend_artifacts::ArticleFailure {
                    url,
                    title,
                    stage: if fetch::is_pdf_url(item.get("url").and_then(Value::as_str).unwrap_or("")) {
                        "unsupported_pdf".to_string()
                    } else {
                        "fetch".to_string()
                    },
                    error: error.to_string(),
                });
                continue;
            }
        };

        let article_body = article_body_from_fetched(&fetched);
        let mut article = merge_recommendation_and_article(item, fetched);
        if let Some(object) = article.as_object_mut() {
            object.insert("article_content".to_string(), json!(article_body));
        }
        let content = recommend_artifacts::format_article_content(&article);
        if let Some(object) = article.as_object_mut() {
            object.insert("content".to_string(), json!(content));
        }
        let json_path = recommend_artifacts::write_article_json(&articles_dir, &stem, &article)?;
        fetched_items.push(article.clone());

        let translated_path = match translate::translate_content(
            article.get("content").and_then(Value::as_str).unwrap_or(""),
        )
        .await
        {
            Ok(translate::TranslateContentOutcome::Translated(markdown)) => {
                translation_attempted = true;
                let path = articles_dir.join(format!("{stem}_translated.md"));
                fs::write(&path, markdown)?;
                translated_items.push(article.clone());
                Some(path)
            }
            Ok(translate::TranslateContentOutcome::Skipped) => None,
            Err(error) => {
                translation_attempted = true;
                failures.push(recommend_artifacts::ArticleFailure {
                    url,
                    title,
                    stage: "translate".to_string(),
                    error: error.to_string(),
                });
                None
            }
        };

        artifacts.push(recommend_artifacts::ArticleArtifact {
            item: article,
            json_path,
            translated_path,
        });
    }

    recommend_artifacts::write_failure_artifact(&outdir, &failures)?;
    ensure_fetch_articles_success(target, translation_attempted, translated_items.len())?;

    if translation_attempted {
        recommend_artifacts::write_translated_index(&outdir, target, &artifacts, !failures.is_empty())?;
    }

    let raw_path = paths::raw_json_path();
    fs::write(&raw_path, serde_json::to_string_pretty(&fetched_items)?)?;
    let seen_items = seen_items_for_fetch_articles(&translated_items, &fetched_items);
    let recorded_count = history.record_seen_items(&seen_items)?;

    eprintln!(
        "Recommended article artifacts collected: {} fetched, {} translated -> {} ({} seen skipped, {} invalid skipped, {} recorded)",
        fetched_items.len(),
        translated_items.len(),
        raw_path.display(),
        dedup_outcome.skipped_seen,
        dedup_outcome.skipped_invalid,
        recorded_count
    );

    Ok(RecommendationCollection {
        item_count: fetched_items.len(),
        source_count: source_count_for_target(target, _config)?,
        raw_path,
        translation_required: false,
    })
}
```

Add the small helper functions used above:

```rust
fn merge_recommendation_and_article(mut recommendation: Value, fetched: Value) -> Value {
    if let (Some(target), Some(source)) = (recommendation.as_object_mut(), fetched.as_object()) {
        for (key, value) in source {
            target.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    recommendation
}

fn article_body_from_fetched(fetched: &Value) -> String {
    fetched
        .get("article_content")
        .or_else(|| fetched.get("content"))
        .or_else(|| fetched.get("text"))
        .or_else(|| fetched.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
```

The `seen_items_for_fetch_articles` helper is the guardrail for the history contract: do not record seen items for skipped or failed translations.

- [ ] **Step 5: Update `main.rs` recommend arm**

Pass the flag:

```rust
let collection = recommend::collect_recommended(
    target,
    limit,
    query.as_deref(),
    &app_config.recommend,
    fetch_articles,
)
.await?;
if collection.translation_required {
    translate::translate(&collection.raw_path).await?;
}
```

- [ ] **Step 6: Run recommend tests**

Run:

```bash
cargo test --locked recommend::tests
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/recommend.rs
git commit -m "feat: fetch and translate recommended articles"
```

---

### Task 6: README Documentation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add command examples**

Add under recommend quick-start examples:

```bash
article-collector recommend all --limit 30 --fetch-articles
article-collector recommend hackernews --limit 10 --fetch-articles
article-collector recommend https://example.com/links --limit 5 --fetch-articles
```

- [ ] **Step 2: Document artifact layout**

Add:

```markdown
`--fetch-articles` を付けると、推薦一覧を取得した後に各 URL の記事本文も取得する。成果物は `ARTICLE_COLLECTOR_OUTDIR` 配下に作成される。

```text
raw.json
translated.md
recommended_articles/
  001-hackernews-example-title.json
  001-hackernews-example-title_translated.md
recommend-fetch-failures.json
```

`translated.md` は結合本文ではなく、記事別翻訳ファイルへの index として作成される。
```

- [ ] **Step 3: Document failure and seen behavior**

Add:

```markdown
本文取得に失敗した URL は `raw.json` や `translated.md` には入れず、`recommend-fetch-failures.json` に `url`, `title`, `stage`, `error` を保存する。`ACP_AGENT` が未設定の場合は記事別 JSON だけを作成し、翻訳ファイル、`translated.md` index、SQLite seen 記録は作成しない。`ACP_AGENT` が設定されている場合、SQLite seen には記事別翻訳まで成功した item だけを記録する。
```

- [ ] **Step 4: Document PDF coming soon**

Add:

```markdown
PDF URL は現時点では本文取得対象外で、`recommend-fetch-failures.json` に `stage: "unsupported_pdf"` として記録される。PDF 本文抽出は coming soon。
```

- [ ] **Step 5: Run README-related checks**

Run:

```bash
cargo test --locked --test cli recommend_help_lists_fetch_articles_flag
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "docs: document recommend article fetching"
```

---

### Task 7: Final Verification For Plan B

**Files:**
- All touched Rust and docs files

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt --check
```

Expected: PASS.

- [ ] **Step 2: Lint**

Run:

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 3: Test**

Run:

```bash
cargo test --locked
```

Expected: PASS.

- [ ] **Step 4: Run live smoke**

Use a temporary output directory and a temporary history path so the user's real history is not polluted:

```bash
$env:ARTICLE_COLLECTOR_OUTDIR = Join-Path $env:TEMP "article-collector-fetch-articles-smoke"
cargo run -- recommend hackernews --limit 1 --fetch-articles --config article-collector.toml
```

Expected when `ACP_AGENT` is not set: command creates `recommended_articles/*.json`, does not create `translated.md`, and does not mark the item seen for translation success.

- [ ] **Step 5: Commit verification fixes if needed**

```bash
git add src/main.rs src/fetch.rs src/translate.rs src/recommend.rs src/recommend_artifacts.rs src/paths.rs tests/cli.rs README.md
git commit -m "chore: polish recommend article fetching"
```
