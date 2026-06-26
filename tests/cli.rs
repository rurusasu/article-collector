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

fn write_recommend_history_config(
    temp_dir: &std::path::Path,
    history_path: &std::path::Path,
) -> std::path::PathBuf {
    let config_path = temp_dir.join("article-collector.toml");
    let history_path_for_toml = history_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &config_path,
        format!("[recommend]\nhistory_path = \"{history_path_for_toml}\"\n"),
    )
    .unwrap();
    config_path
}

fn insert_seen_recommend_history_item(history_path: &std::path::Path) {
    let conn = rusqlite::Connection::open(history_path).unwrap();
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
}

fn recommend_history_count(history_path: &std::path::Path) -> i64 {
    let conn = rusqlite::Connection::open(history_path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM recommend_seen_items", [], |row| {
        row.get(0)
    })
    .unwrap()
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
    let config_path = write_recommend_history_config(&temp_dir, &history_path);
    insert_seen_recommend_history_item(&history_path);

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

    assert_eq!(recommend_history_count(&history_path), 0);

    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn history_clear_keeps_env_scoped_temp_and_output_dirs() {
    let sandbox = unique_temp_path("history-clear-artifacts");
    let artifact_temp_dir = sandbox.join("temp-artifacts");
    let output_dir = sandbox.join("final-output");
    std::fs::create_dir_all(&artifact_temp_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();
    let raw_json = artifact_temp_dir.join("raw.json");
    let final_markdown = output_dir.join("hackernews-news").join("article.md");
    std::fs::create_dir_all(final_markdown.parent().unwrap()).unwrap();
    std::fs::write(&raw_json, "[]").unwrap();
    std::fs::write(&final_markdown, "# article").unwrap();

    let history_path = sandbox.join("recommend-history.sqlite");
    let config_path = write_recommend_history_config(&sandbox, &history_path);
    insert_seen_recommend_history_item(&history_path);

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .env("ARTICLE_COLLECTOR_TEMP_DIR", &artifact_temp_dir)
        .env("ARTICLE_COLLECTOR_OUTPUT_DIR", &output_dir)
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

    assert_eq!(recommend_history_count(&history_path), 0);
    assert!(
        raw_json.exists(),
        "history clear should keep ARTICLE_COLLECTOR_TEMP_DIR files"
    );
    assert!(
        final_markdown.exists(),
        "history clear should keep ARTICLE_COLLECTOR_OUTPUT_DIR files"
    );

    std::fs::remove_dir_all(sandbox).unwrap();
}

#[test]
fn history_clear_all_uses_configured_history_path_and_removes_seen_items() {
    let temp_dir = unique_temp_path("history-clear-all-cli");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let history_path = temp_dir.join("recommend-history.sqlite");
    let config_path = write_recommend_history_config(&temp_dir, &history_path);
    insert_seen_recommend_history_item(&history_path);

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .args([
            "history",
            "clear",
            "all",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector history clear all");

    assert!(
        output.status.success(),
        "expected history clear all to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Cleared 1 recommend history item(s)"),
        "stderr should report cleared count, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(recommend_history_count(&history_path), 0);

    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn history_clear_all_removes_env_scoped_temp_and_output_dirs() {
    let sandbox = unique_temp_path("history-clear-all-artifacts");
    let artifact_temp_dir = sandbox.join("temp-artifacts");
    let output_dir = sandbox.join("final-output");
    let nested_temp_dir = artifact_temp_dir.join("recommended_articles").join("hn");
    let nested_output_dir = output_dir.join("hackernews-news");
    std::fs::create_dir_all(&nested_temp_dir).unwrap();
    std::fs::create_dir_all(&nested_output_dir).unwrap();
    std::fs::write(artifact_temp_dir.join("raw.json"), "[]").unwrap();
    std::fs::write(artifact_temp_dir.join("translated.md"), "# translated").unwrap();
    std::fs::write(nested_temp_dir.join("article.json"), "{}").unwrap();
    std::fs::write(nested_output_dir.join("article.md"), "# article").unwrap();

    let history_path = sandbox.join("recommend-history.sqlite");
    let config_path = write_recommend_history_config(&sandbox, &history_path);
    insert_seen_recommend_history_item(&history_path);

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .env("ARTICLE_COLLECTOR_TEMP_DIR", &artifact_temp_dir)
        .env("ARTICLE_COLLECTOR_OUTPUT_DIR", &output_dir)
        .args([
            "history",
            "clear",
            "all",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector history clear all");

    assert!(
        output.status.success(),
        "expected history clear all to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(recommend_history_count(&history_path), 0);
    assert!(
        !artifact_temp_dir.exists(),
        "ARTICLE_COLLECTOR_TEMP_DIR should be removed"
    );
    assert!(
        !output_dir.exists(),
        "ARTICLE_COLLECTOR_OUTPUT_DIR should be removed"
    );
    assert!(
        sandbox.exists(),
        "history clear all should not remove parents"
    );

    std::fs::remove_dir_all(sandbox).unwrap();
}

#[test]
fn history_clear_all_without_artifact_env_keeps_default_temp_dir() {
    let sandbox = unique_temp_path("history-clear-all-no-env");
    std::fs::create_dir_all(&sandbox).unwrap();
    let history_path = sandbox.join("recommend-history.sqlite");
    let config_path = write_recommend_history_config(&sandbox, &history_path);
    insert_seen_recommend_history_item(&history_path);
    let default_temp_dir = std::env::temp_dir().join("article-collector");
    std::fs::create_dir_all(&default_temp_dir).unwrap();
    let marker = default_temp_dir.join(format!("history-clear-all-marker-{}", std::process::id()));
    std::fs::write(&marker, "keep").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .env_remove("ARTICLE_COLLECTOR_TEMP_DIR")
        .env_remove("ARTICLE_COLLECTOR_OUTPUT_DIR")
        .args([
            "history",
            "clear",
            "all",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector history clear all");

    assert!(
        output.status.success(),
        "expected history clear all to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(recommend_history_count(&history_path), 0);
    assert!(
        marker.exists(),
        "default temp dir must not be deleted when ARTICLE_COLLECTOR_TEMP_DIR is unset"
    );

    std::fs::remove_file(marker).unwrap();
    std::fs::remove_dir_all(sandbox).unwrap();
}

#[test]
#[ignore = "requires live network access and configured Codex ACP agent"]
fn recommend_all_then_history_clear_all_e2e() {
    let sandbox = unique_temp_path("recommend-clear-all-e2e");
    let artifact_temp_dir = sandbox.join("temp-artifacts");
    let output_dir = sandbox.join("final-output");
    std::fs::create_dir_all(&artifact_temp_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();
    let history_path = sandbox.join("recommend-history.sqlite");
    let config_path = write_recommend_history_config(&sandbox, &history_path);

    let recommend_output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .env("ARTICLE_COLLECTOR_TEMP_DIR", &artifact_temp_dir)
        .env("ARTICLE_COLLECTOR_OUTPUT_DIR", &output_dir)
        .env("ACP_AGENT", "codex")
        .env("TRANSLATE_LANG", "ja")
        .args([
            "recommend",
            "all",
            "--limit",
            "5",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector recommend all");

    assert!(
        recommend_output.status.success(),
        "expected recommend all e2e to succeed, stderr: {}",
        String::from_utf8_lossy(&recommend_output.stderr)
    );
    assert!(
        artifact_temp_dir.join("raw.json").exists(),
        "recommend all should create raw.json"
    );

    let clear_output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .env("ARTICLE_COLLECTOR_TEMP_DIR", &artifact_temp_dir)
        .env("ARTICLE_COLLECTOR_OUTPUT_DIR", &output_dir)
        .args([
            "history",
            "clear",
            "all",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector history clear all");

    assert!(
        clear_output.status.success(),
        "expected history clear all e2e cleanup to succeed, stderr: {}",
        String::from_utf8_lossy(&clear_output.stderr)
    );
    assert_eq!(recommend_history_count(&history_path), 0);
    assert!(
        !artifact_temp_dir.exists(),
        "history clear all should remove ARTICLE_COLLECTOR_TEMP_DIR"
    );
    assert!(
        !output_dir.exists(),
        "history clear all should remove ARTICLE_COLLECTOR_OUTPUT_DIR"
    );

    std::fs::remove_dir_all(sandbox).unwrap();
}
