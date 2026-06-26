use std::process::Command;

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
fn root_version_flag_prints_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .arg("--version")
        .output()
        .expect("run article-collector --version");

    assert!(
        output.status.success(),
        "expected --version to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("version output should be valid UTF-8");
    assert_eq!(
        stdout.trim(),
        format!("article-collector {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        output.stderr.is_empty(),
        "expected no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn root_help_lists_save_and_pr_but_not_save_and_pr() {
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
        stdout.contains("save "),
        "help should list save command:\n{stdout}"
    );
    assert!(
        stdout.contains("pr "),
        "help should list pr command:\n{stdout}"
    );
    assert!(
        !stdout.contains("save-and-pr"),
        "help should not list removed save-and-pr command:\n{stdout}"
    );
}

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

#[test]
fn recommend_help_does_not_list_fetch_articles_flag() {
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
        !stdout.contains("--fetch-articles"),
        "recommend help should not list removed --fetch-articles flag:\n{stdout}"
    );
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

    let conn = rusqlite::Connection::open(&history_path).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM recommend_seen_items", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    drop(conn);

    std::fs::remove_dir_all(temp_dir).unwrap();
}
