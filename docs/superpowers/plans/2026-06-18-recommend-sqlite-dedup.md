# Recommend SQLite Dedup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SQLite-backed persistent deduplication to `article-collector recommend` while keeping `raw.json` as the current-run output.

**Architecture:** Add a focused `recommend_history` module that owns SQLite schema setup, URL key normalization, seen-item filtering, and history inserts. Keep `recommend.rs` responsible for source collection and artifact writing, inserting the history filter between candidate collection and `raw.json` writing.

**Tech Stack:** Rust stable, rusqlite with bundled SQLite, serde/serde_json, reqwest URL parser, chrono, dirs, TOML config.

---

## File Structure

- `Cargo.toml`: add `rusqlite` with bundled SQLite.
- `Cargo.lock`: update through Cargo after adding `rusqlite`.
- `src/config.rs`: add `[recommend].history_path` parsing.
- `src/recommend_history.rs`: new SQLite history store, URL normalization, filtering, and unit tests.
- `src/main.rs`: register the new module.
- `src/recommend.rs`: apply history filtering before writing `raw.json`, then record emitted items.
- `README.md`: document SQLite history behavior, `history_path`, and all-seen non-zero exits.
- `article-collector.toml`: show optional `history_path` as a commented example.

## Scope Check

The approved spec covers one subsystem: recommend deduplication. The plan keeps it as one implementation flow because every task contributes to the same command path and can be verified locally without external services.

---

### Task 1: Config Support For History Path

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write the failing config test**

Add this test inside `src/config.rs` `#[cfg(test)] mod tests`:

```rust
/// 検証: recommend history DB path を TOML から読める
/// 理由: cron や手動実行で同じ SQLite 履歴を明示的に共有したい
/// リスク: outdir が変わるたびに重複排除が効かなくなる
#[test]
fn parses_recommend_history_path() {
    let config = parse_config(
        r#"
        [recommend]
        history_path = "D:/article-collector-data/recommend-history.sqlite"
        "#,
    )
    .unwrap();

    assert_eq!(
        config.recommend.history_path,
        Some(std::path::PathBuf::from(
            "D:/article-collector-data/recommend-history.sqlite"
        ))
    );
}
```

- [ ] **Step 2: Run the config test and verify it fails**

Run:

```bash
cargo test --locked config::tests::parses_recommend_history_path
```

Expected: FAIL to compile with a missing `history_path` field on `RecommendConfig`.

- [ ] **Step 3: Add the config field**

In `src/config.rs`, update `RecommendConfig`:

```rust
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RecommendConfig {
    pub limit: Option<usize>,
    pub sources: Option<Vec<String>>,
    pub history_path: Option<PathBuf>,
    pub source: BTreeMap<String, RecommendSourceConfig>,
}
```

- [ ] **Step 4: Run the config test and verify it passes**

Run:

```bash
cargo test --locked config::tests::parses_recommend_history_path
```

Expected: PASS.

- [ ] **Step 5: Commit config support**

Run:

```bash
git add src/config.rs
git commit -m "feat: parse recommend history path"
```

---

### Task 2: SQLite History Module

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/main.rs`
- Create: `src/recommend_history.rs`

- [ ] **Step 1: Add the SQLite dependency**

In `Cargo.toml`, add this dependency near the other data/storage crates:

```toml
rusqlite = { version = "0.40", features = ["bundled"] }
```

- [ ] **Step 2: Register the history module**

In `src/main.rs`, add the module declaration:

```rust
mod recommend_history;
```

- [ ] **Step 3: Write the failing history tests**

Create `src/recommend_history.rs` with these tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_url_removes_fragment_and_preserves_query() {
        assert_eq!(
            canonical_recommend_url(" HTTPS://Example.COM/posts/One?utm_source=zenn&id=1#comments "),
            Some("https://example.com/posts/One?utm_source=zenn&id=1".to_string())
        );
    }

    #[test]
    fn rejects_non_http_urls_for_dedupe_keys() {
        assert_eq!(canonical_recommend_url("mailto:test@example.com"), None);
        assert_eq!(canonical_recommend_url("not a url"), None);
    }

    #[test]
    fn initializes_sqlite_schema() {
        let store = RecommendationHistory::in_memory_for_tests().unwrap();

        let table_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'recommend_seen_items'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let user_version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();

        assert_eq!(table_count, 1);
        assert_eq!(user_version, 1);
    }

    #[test]
    fn filters_seen_items_and_preserves_unseen_order() {
        let mut store = RecommendationHistory::in_memory_for_tests().unwrap();
        let seen = json!({
            "source": "zenn",
            "site": "zenn",
            "title": "Already seen",
            "url": "https://zenn.dev/example/articles/seen#comments"
        });
        let first_new = json!({
            "source": "zenn",
            "site": "zenn",
            "title": "First new",
            "url": "https://zenn.dev/example/articles/new-1"
        });
        let second_new = json!({
            "source": "devto",
            "site": "devto",
            "title": "Second new",
            "url": "https://dev.to/example/new-2"
        });

        store.record_seen_items(&[seen.clone()]).unwrap();
        let outcome = store
            .filter_new_items(vec![seen, first_new.clone(), second_new.clone()])
            .unwrap();

        assert_eq!(outcome.items, vec![first_new, second_new]);
        assert_eq!(outcome.skipped_seen, 1);
        assert_eq!(outcome.skipped_invalid, 0);
    }

    #[test]
    fn inserts_only_the_items_passed_to_record_seen_items() {
        let mut store = RecommendationHistory::in_memory_for_tests().unwrap();
        let emitted = json!({
            "source": "hackernews",
            "site": "hackernews",
            "title": "Emitted",
            "url": "https://example.com/emitted"
        });
        let not_emitted_key = canonical_recommend_url("https://example.com/not-emitted").unwrap();

        let inserted = store.record_seen_items(&[emitted]).unwrap();

        assert_eq!(inserted, 1);
        assert!(store.contains_key("https://example.com/emitted").unwrap());
        assert!(!store.contains_key(&not_emitted_key).unwrap());
    }

    #[test]
    fn skips_items_with_invalid_urls() {
        let store = RecommendationHistory::in_memory_for_tests().unwrap();
        let outcome = store
            .filter_new_items(vec![json!({
                "source": "generic-web",
                "title": "Broken",
                "url": "not a url"
            })])
            .unwrap();

        assert!(outcome.items.is_empty());
        assert_eq!(outcome.skipped_seen, 0);
        assert_eq!(outcome.skipped_invalid, 1);
    }
}
```

- [ ] **Step 4: Run the history tests and verify they fail**

Run:

```bash
cargo test --locked recommend_history::tests
```

Expected: FAIL to compile because `canonical_recommend_url` and `RecommendationHistory` do not exist yet.

- [ ] **Step 5: Implement the history module**

Add this implementation above the tests in `src/recommend_history.rs`:

```rust
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Url;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;
const HISTORY_DIR: &str = "article-collector";
const HISTORY_FILE: &str = "recommend-history.sqlite";

#[derive(Debug, PartialEq, Eq)]
pub struct DedupOutcome {
    pub items: Vec<Value>,
    pub skipped_seen: usize,
    pub skipped_invalid: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeenRecommendation {
    dedupe_key: String,
    canonical_url: String,
    original_url: String,
    source: String,
    site: Option<String>,
    title: Option<String>,
}

pub struct RecommendationHistory {
    conn: Connection,
}

impl RecommendationHistory {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create recommend history directory {}", parent.display())
            })?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open recommend history DB {}", path.display()))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    #[cfg(test)]
    fn in_memory_for_tests() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS recommend_seen_items (
              dedupe_key TEXT PRIMARY KEY,
              canonical_url TEXT NOT NULL,
              original_url TEXT NOT NULL,
              source TEXT NOT NULL,
              site TEXT,
              title TEXT,
              first_seen_at TEXT NOT NULL,
              last_seen_at TEXT NOT NULL
            );
            PRAGMA user_version = 1;
            "#,
        )?;
        Ok(())
    }

    pub fn filter_new_items(&self, items: Vec<Value>) -> Result<DedupOutcome> {
        let mut filtered = Vec::new();
        let mut skipped_seen = 0;
        let mut skipped_invalid = 0;

        for item in items {
            let Some(record) = seen_recommendation_from_item(&item) else {
                skipped_invalid += 1;
                continue;
            };

            if self.contains_key(&record.dedupe_key)? {
                skipped_seen += 1;
                continue;
            }

            filtered.push(item);
        }

        Ok(DedupOutcome {
            items: filtered,
            skipped_seen,
            skipped_invalid,
        })
    }

    pub fn record_seen_items(&mut self, items: &[Value]) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction()?;
        let mut inserted = 0;

        {
            let mut statement = tx.prepare(
                r#"
                INSERT OR IGNORE INTO recommend_seen_items (
                  dedupe_key,
                  canonical_url,
                  original_url,
                  source,
                  site,
                  title,
                  first_seen_at,
                  last_seen_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                "#,
            )?;

            for item in items {
                let Some(record) = seen_recommendation_from_item(item) else {
                    continue;
                };

                inserted += statement.execute(params![
                    record.dedupe_key,
                    record.canonical_url,
                    record.original_url,
                    record.source,
                    record.site,
                    record.title,
                    now,
                ])?;
            }
        }

        tx.commit()?;
        Ok(inserted)
    }

    pub fn contains_key(&self, dedupe_key: &str) -> Result<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM recommend_seen_items WHERE dedupe_key = ?1)",
            [dedupe_key],
            |row| row.get(0),
        )?;
        Ok(exists != 0)
    }
}

pub fn default_history_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context(
        "Could not resolve user config directory; set [recommend].history_path explicitly",
    )?;
    Ok(base.join(HISTORY_DIR).join(HISTORY_FILE))
}

pub fn canonical_recommend_url(raw_url: &str) -> Option<String> {
    let mut url = Url::parse(raw_url.trim()).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }

    url.set_fragment(None);
    Some(url.to_string())
}

fn seen_recommendation_from_item(item: &Value) -> Option<SeenRecommendation> {
    let original_url = item.get("url")?.as_str()?.trim().to_string();
    if original_url.is_empty() {
        return None;
    }

    let canonical_url = canonical_recommend_url(&original_url)?;
    let source = string_field(item, "source")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    Some(SeenRecommendation {
        dedupe_key: canonical_url.clone(),
        canonical_url,
        original_url,
        source,
        site: string_field(item, "site").filter(|value| !value.is_empty()),
        title: string_field(item, "title").filter(|value| !value.is_empty()),
    })
}

fn string_field(item: &Value, field: &str) -> Option<String> {
    item.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .map(ToOwned::to_owned)
}
```

- [ ] **Step 6: Run the history tests and verify they pass**

Run:

```bash
cargo test --locked recommend_history::tests
```

Expected: PASS.

- [ ] **Step 7: Commit the history module**

Run:

```bash
git add Cargo.toml Cargo.lock src/main.rs src/recommend_history.rs
git commit -m "feat: add recommend history store"
```

---

### Task 3: Apply Deduplication In Recommend Flow

**Files:**
- Modify: `src/recommend.rs`

- [ ] **Step 1: Write failing recommend-flow tests**

In `src/recommend.rs` tests module, add these tests:

```rust
/// 検証: 新規推薦が 0 件なら明示的なエラーにする
/// 理由: 既読しかない実行で翻訳や後続処理へ進まないようにする
/// リスク: 空の raw.json を成功として扱い、cron 結果が曖昧になる
#[test]
fn rejects_empty_new_recommendations() {
    let error = ensure_new_recommendations("all", &[]).unwrap_err();

    assert_eq!(
        error.to_string(),
        "No new recommended articles found for all"
    );
}

/// 検証: config に履歴 DB path があればそれを優先する
/// 理由: cron と手動実行で同じ履歴を共有したい
/// リスク: default path だけに依存して環境差分を吸収できない
#[test]
fn history_path_prefers_config_value() {
    let config = RecommendConfig {
        history_path: Some(PathBuf::from("D:/article-collector-data/history.sqlite")),
        ..Default::default()
    };

    assert_eq!(
        history_path_for_config(&config).unwrap(),
        PathBuf::from("D:/article-collector-data/history.sqlite")
    );
}
```

Also update the test imports in `src/recommend.rs` tests module:

```rust
use std::path::PathBuf;
```

- [ ] **Step 2: Run the recommend-flow tests and verify they fail**

Run:

```bash
cargo test --locked recommend::tests::rejects_empty_new_recommendations
cargo test --locked recommend::tests::history_path_prefers_config_value
```

Expected: FAIL to compile because `ensure_new_recommendations`, `history_path_for_config`, and the `recommend_history` module integration do not exist yet.

- [ ] **Step 3: Add recommend history helpers**

In `src/recommend.rs`, add this import near the existing crate imports:

```rust
use crate::recommend_history::{default_history_path, RecommendationHistory};
```

Add these helper functions near `source_count_for_target`:

```rust
fn history_path_for_config(config: &RecommendConfig) -> Result<PathBuf> {
    config
        .history_path
        .clone()
        .map(Ok)
        .unwrap_or_else(default_history_path)
}

fn ensure_new_recommendations(target: &str, items: &[Value]) -> Result<()> {
    if items.is_empty() {
        bail!("No new recommended articles found for {target}");
    }
    Ok(())
}
```

- [ ] **Step 4: Filter collected candidates before writing `raw.json`**

In `src/recommend.rs`, change `collect_recommended` so the collected candidate list is mutable, then apply SQLite filtering before the existing empty check and artifact write:

```rust
    let mut items = match recommendation_target {
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

    let history_path = history_path_for_config(config)?;
    let mut history = RecommendationHistory::open(&history_path)?;
    let dedup_outcome = history.filter_new_items(items)?;
    items = dedup_outcome.items;

    ensure_new_recommendations(target, &items)?;
```

After the existing `fs::write(&raw_path, serde_json::to_string_pretty(&items)?)?;`, insert:

```rust
    let recorded_count = history.record_seen_items(&items)?;
```

Replace the existing success `eprintln!` with:

```rust
    eprintln!(
        "Recommended articles collected: {} new item(s) -> {} ({} seen skipped, {} invalid skipped, {} recorded)",
        items.len(),
        raw_path.display(),
        dedup_outcome.skipped_seen,
        dedup_outcome.skipped_invalid,
        recorded_count
    );
```

- [ ] **Step 5: Run the recommend-flow tests and verify they pass**

Run:

```bash
cargo test --locked recommend::tests::rejects_empty_new_recommendations
cargo test --locked recommend::tests::history_path_prefers_config_value
```

Expected: PASS.

- [ ] **Step 6: Run non-network recommend/history unit tests**

Run:

```bash
cargo test --locked config::tests::parses_recommend_history_path
cargo test --locked recommend_history::tests
cargo test --locked recommend::tests::rejects_empty_new_recommendations
cargo test --locked recommend::tests::history_path_prefers_config_value
```

Expected: PASS.

- [ ] **Step 7: Commit recommend integration**

Run:

```bash
git add src/recommend.rs
git commit -m "feat: dedupe recommend results with history"
```

---

### Task 4: Documentation And Example Config

**Files:**
- Modify: `README.md`
- Modify: `article-collector.toml`

- [ ] **Step 1: Update the TOML example file**

In `article-collector.toml`, add this commented example under `[recommend]`:

```toml
# history_path = "D:/article-collector-data/recommend-history.sqlite"
```

- [ ] **Step 2: Update README config example**

In `README.md` `### TOML config`, update the example:

```toml
[recommend]
sources = ["hackernews", "devto", "zenn", "arxiv"]
limit = 30
# history_path = "D:/article-collector-data/recommend-history.sqlite"

[recommend.source.arxiv]
limit = 10
query = "cat:cs.AI OR cat:cs.CL OR cat:cs.CV OR cat:cs.LG OR cat:cs.IR OR cat:cs.SE OR cat:stat.ML"
```

- [ ] **Step 3: Document recommend dedup behavior**

In `README.md` `### レコメンド収集`, add this paragraph after the `recommend all` paragraph:

```markdown
`recommend` は SQLite の既読履歴で過去に `raw.json` へ出力した記事 URL を管理し、次回以降は同じ記事を `raw.json` に出力しない。履歴 DB は既定ではユーザー設定ディレクトリ配下の `article-collector/recommend-history.sqlite` に作成する。別の場所を使う場合は `[recommend].history_path` を指定する。全候補が既読だった場合は `No new recommended articles found for <target>` で非ゼロ終了する。
```

- [ ] **Step 4: Run TOML formatting/lint check if Taplo is available**

Run:

```bash
task toml-check
```

Expected: PASS when `taplo` is installed. If `taplo` is not installed, record the precondition failure and continue to the Rust verification task.

- [ ] **Step 5: Commit docs**

Run:

```bash
git add README.md article-collector.toml
git commit -m "docs: document recommend history"
```

---

### Task 5: Final Verification

**Files:**
- Verify only; no planned source changes.

- [ ] **Step 1: Format Rust code**

Run:

```bash
cargo fmt
```

Expected: command exits 0.

- [ ] **Step 2: Check Rust formatting**

Run:

```bash
cargo fmt --check
```

Expected: PASS.

- [ ] **Step 3: Run clippy**

Run:

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --locked
```

Expected: PASS. If the existing live-source test fails due to network or a remote site change, capture the exact failing test and error, then run the non-network subset from Task 3 Step 6 to verify the SQLite change itself.

- [ ] **Step 5: Build release binary**

Run:

```bash
cargo build --release --locked
```

Expected: PASS.

- [ ] **Step 6: Check final git status**

Run:

```bash
git status --short
```

Expected: only intentional files are modified or the worktree is clean after commits. `.codex/` and `AGENTS.md` may remain untracked from pre-existing local state and should not be added unless the user explicitly asks.
