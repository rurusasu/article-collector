# History Clear Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `article-collector history clear` to clear SQLite recommend deduplication history.

**Architecture:** Add a top-level `history` CLI namespace in `src/main.rs` with a single `clear` subcommand. Reuse the existing config loader and recommend history path resolution, and add a small clear method to `RecommendationHistory` so the database layer owns the table operation.

**Tech Stack:** Rust stable, clap nested subcommands, rusqlite, existing config and recommend history modules.

---

## File Structure

- Modify `src/recommend_history.rs`: add `clear_seen_items` and a unit test.
- Modify `src/recommend.rs`: expose the existing history path resolver to the crate.
- Modify `src/main.rs`: add `history clear --config <PATH>` parsing and dispatch.
- Modify `tests/cli.rs`: add CLI help and end-to-end clear behavior tests.
- Modify `README.md`: document the command.

### Task 1: Add history clearing to the storage layer

**Files:**
- Modify: `src/recommend_history.rs`

- [ ] **Step 1: Write the failing storage test**

Add this test to `src/recommend_history.rs`:

```rust
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

    store.record_seen_items(&[first.clone(), second.clone()]).unwrap();

    let cleared = store.clear_seen_items().unwrap();
    let outcome = store.filter_new_items(vec![first, second]).unwrap();

    assert_eq!(cleared, 2);
    assert_eq!(outcome.items.len(), 2);
    assert_eq!(outcome.skipped_seen, 0);
    assert_eq!(outcome.skipped_invalid, 0);
}
```

- [ ] **Step 2: Verify the test fails**

Run:

```bash
cargo test --locked recommend_history::tests::clears_all_seen_items_and_reports_deleted_count
```

Expected: compile failure because `clear_seen_items` does not exist.

- [ ] **Step 3: Implement the minimal storage method**

Add this method inside `impl RecommendationHistory`:

```rust
pub fn clear_seen_items(&mut self) -> Result<usize> {
    let deleted = self.conn.execute("DELETE FROM recommend_seen_items", [])?;
    Ok(deleted)
}
```

- [ ] **Step 4: Verify the storage test passes**

Run:

```bash
cargo test --locked recommend_history::tests::clears_all_seen_items_and_reports_deleted_count
```

Expected: PASS.

### Task 2: Add the CLI namespace and dispatch

**Files:**
- Modify: `src/recommend.rs`
- Modify: `src/main.rs`
- Modify: `tests/cli.rs`

- [ ] **Step 1: Write the failing CLI help test**

Add this test to `tests/cli.rs`:

```rust
#[test]
fn root_help_lists_history_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .arg("--help")
        .output()
        .expect("run article-collector --help");

    assert!(
        output.status.success(),
        "expected --help to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("help output should be valid UTF-8");
    assert!(
        stdout.contains("history"),
        "help should list history command:\n{stdout}"
    );
}
```

- [ ] **Step 2: Verify the CLI help test fails**

Run:

```bash
cargo test --locked --test cli root_help_lists_history_command
```

Expected: FAIL because root help does not list `history`.

- [ ] **Step 3: Expose history path resolution**

Change `history_path_for_config` in `src/recommend.rs` to:

```rust
pub(crate) fn history_path_for_config(config: &RecommendConfig) -> Result<PathBuf> {
    config
        .history_path
        .clone()
        .map(Ok)
        .unwrap_or_else(default_history_path)
}
```

- [ ] **Step 4: Add nested history commands**

Update the clap imports in `src/main.rs`:

```rust
use clap::{Parser, Subcommand};
```

Add these command variants:

```rust
#[derive(Subcommand)]
enum Commands {
    // existing variants stay unchanged
    /// Recommend history maintenance
    History {
        #[command(subcommand)]
        command: HistoryCommands,
    },
}

#[derive(Subcommand)]
enum HistoryCommands {
    /// Clear SQLite recommend deduplication history
    Clear {
        /// article-collector TOML config のパス
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
}
```

Add this match arm in `main`:

```rust
Commands::History {
    command: HistoryCommands::Clear { ref config },
} => {
    let app_config = config::load(config.as_deref())?;
    let history_path = recommend::history_path_for_config(&app_config.recommend)?;
    let mut history = recommend_history::RecommendationHistory::open(&history_path)?;
    let cleared = history.clear_seen_items()?;
    eprintln!(
        "Cleared {cleared} recommend history item(s) from {}",
        history_path.display()
    );
}
```

- [ ] **Step 5: Verify the CLI help test passes**

Run:

```bash
cargo test --locked --test cli root_help_lists_history_command
```

Expected: PASS.

### Task 3: Add an end-to-end CLI behavior test

**Files:**
- Modify: `tests/cli.rs`

- [ ] **Step 1: Write the failing clear behavior test**

Add helper functions and this test to `tests/cli.rs`:

```rust
fn unique_temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "article-collector-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn history_clear_uses_configured_history_path_and_removes_seen_items() {
    let temp_dir = unique_temp_path("history-clear-cli");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let history_path = temp_dir.join("recommend-history.sqlite");
    let config_path = temp_dir.join("article-collector.toml");
    let history_path_for_toml = history_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &config_path,
        format!("[recommend]\nhistory_path = \"{history_path_for_toml}\"\n"),
    )
    .unwrap();

    let mut store = article_collector_test_support_open_history(&history_path);
    let item = serde_json::json!({
        "source": "hackernews",
        "site": "hackernews",
        "title": "Already seen",
        "url": "https://example.com/already-seen"
    });
    store.record_seen_items(std::slice::from_ref(&item)).unwrap();
    assert!(store.contains_key("https://example.com/already-seen").unwrap());
    drop(store);

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .args([
            "history",
            "clear",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector history clear");

    assert!(
        output.status.success(),
        "expected history clear to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Cleared 1 recommend history item(s)"),
        "stderr should report cleared count, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let store = article_collector_test_support_open_history(&history_path);
    assert!(!store.contains_key("https://example.com/already-seen").unwrap());

    std::fs::remove_dir_all(temp_dir).unwrap();
}
```

Use a direct SQLite query in the final implementation instead of a test-only production API if importing crate internals is not available from the integration test.

- [ ] **Step 2: Verify the behavior test fails**

Run:

```bash
cargo test --locked --test cli history_clear_uses_configured_history_path_and_removes_seen_items
```

Expected: FAIL until the test is adapted to the integration-test boundary and the command exists.

- [ ] **Step 3: Make the integration test use rusqlite directly**

Because `tests/cli.rs` cannot import private crate internals, insert and verify rows with `rusqlite::Connection`:

```rust
let conn = rusqlite::Connection::open(&history_path).unwrap();
conn.execute_batch(
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
    INSERT INTO recommend_seen_items (
      dedupe_key, canonical_url, original_url, source, site, title, first_seen_at, last_seen_at
    )
    VALUES (
      'https://example.com/already-seen',
      'https://example.com/already-seen',
      'https://example.com/already-seen',
      'hackernews',
      'hackernews',
      'Already seen',
      '2026-06-26T00:00:00Z',
      '2026-06-26T00:00:00Z'
    );
    "#,
)
.unwrap();
drop(conn);
```

After the command, verify:

```rust
let conn = rusqlite::Connection::open(&history_path).unwrap();
let count: i64 = conn
    .query_row("SELECT COUNT(*) FROM recommend_seen_items", [], |row| row.get(0))
    .unwrap();
assert_eq!(count, 0);
```

- [ ] **Step 4: Verify the behavior test passes**

Run:

```bash
cargo test --locked --test cli history_clear_uses_configured_history_path_and_removes_seen_items
```

Expected: PASS.

### Task 4: Document and run full verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README command table**

Add a row near the recommend commands:

```markdown
| `article-collector history clear --config article-collector.toml` | SQLite recommend history を clear | 次回以降の `recommend` で既読記事も再出力可能にする |
```

- [ ] **Step 2: Update recommend history section**

Add this sentence after the paragraph that explains SQLite history:

```markdown
SQLite 履歴を消して再収集したい場合は `article-collector history clear` を実行する。`--config` を指定すると `[recommend].history_path` の DB を clear する。
```

- [ ] **Step 3: Format and verify**

Run:

```bash
cargo fmt
cargo clippy --locked
cargo test --locked
```

Expected: all commands exit 0.

- [ ] **Step 4: Review final diff**

Run:

```bash
git -c safe.directory=D:/article-collector diff -- src/recommend_history.rs src/recommend.rs src/main.rs tests/cli.rs README.md docs/superpowers/specs/2026-06-26-history-clear-design.md docs/superpowers/plans/2026-06-26-history-clear.md
```

Expected: diff only covers `history clear` implementation, tests, and docs.
