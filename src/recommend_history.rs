use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Url;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashSet;
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
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create recommend history directory {}",
                    parent.display()
                )
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
        let schema = format!(
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
            PRAGMA user_version = {SCHEMA_VERSION};
            "#
        );
        self.conn.execute_batch(&schema)?;
        Ok(())
    }

    pub fn filter_new_items(&self, items: Vec<Value>) -> Result<DedupOutcome> {
        let mut filtered = Vec::new();
        let mut accepted_keys: HashSet<String> = HashSet::new();
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

            if !accepted_keys.insert(record.dedupe_key) {
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

    pub fn clear_seen_items(&mut self) -> Result<usize> {
        let deleted = self.conn.execute("DELETE FROM recommend_seen_items", [])?;
        Ok(deleted)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_url_removes_fragment_and_preserves_query() {
        assert_eq!(
            canonical_recommend_url(
                " HTTPS://Example.COM/posts/One?utm_source=zenn&id=1#comments "
            ),
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

        store
            .record_seen_items(std::slice::from_ref(&seen))
            .unwrap();
        let outcome = store
            .filter_new_items(vec![seen, first_new.clone(), second_new.clone()])
            .unwrap();

        assert_eq!(outcome.items, vec![first_new, second_new]);
        assert_eq!(outcome.skipped_seen, 1);
        assert_eq!(outcome.skipped_invalid, 0);
    }

    #[test]
    fn filter_new_items_dedupes_duplicate_canonical_urls_in_batch() {
        let store = RecommendationHistory::in_memory_for_tests().unwrap();
        let first = json!({
            "source": "generic-web",
            "site": "example",
            "title": "First fragment",
            "url": "https://example.com/story#one"
        });
        let duplicate = json!({
            "source": "generic-web",
            "site": "example",
            "title": "Second fragment",
            "url": "https://example.com/story#two"
        });
        let later = json!({
            "source": "generic-web",
            "site": "example",
            "title": "Later unique",
            "url": "https://example.com/later"
        });

        let outcome = store
            .filter_new_items(vec![first.clone(), duplicate, later.clone()])
            .unwrap();

        assert_eq!(outcome.items, vec![first, later]);
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
    fn clears_all_seen_items_and_reports_deleted_count() {
        let mut store = RecommendationHistory::in_memory_for_tests().unwrap();
        let first = json!({
            "source": "hackernews",
            "site": "hackernews",
            "title": "First",
            "url": "https://example.com/first"
        });
        let second = json!({
            "source": "devto",
            "site": "devto",
            "title": "Second",
            "url": "https://example.com/second"
        });

        store
            .record_seen_items(&[first.clone(), second.clone()])
            .unwrap();

        let cleared = store.clear_seen_items().unwrap();
        let outcome = store.filter_new_items(vec![first, second]).unwrap();

        assert_eq!(cleared, 2);
        assert_eq!(outcome.items.len(), 2);
        assert_eq!(outcome.skipped_seen, 0);
        assert_eq!(outcome.skipped_invalid, 0);
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
