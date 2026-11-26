use std::process::Command;

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
fn root_help_lists_public_command_groups() {
    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .arg("--help")
        .output()
        .expect("run article-collector --help");

    assert!(output.status.success(), "help failed: {:?}", output.stderr);
    let help = String::from_utf8(output.stdout).unwrap();
    let commands = help
        .split_once("Commands:\n")
        .expect("help should contain the public commands")
        .1
        .lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    assert_eq!(commands, ["article", "history", "plugin", "help"]);
}

#[tokio::test]
async fn article_collect_outputs_candidates_or_bodies_at_the_requested_stage() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            stream.read_line(&mut request).await.unwrap();
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/seed" => r#"<a href="/article">Example candidate</a>"#,
                "/article" => "<title>Example article</title><p>Fixture body</p>",
                path => panic!("unexpected fixture path {path}"),
            };
            let mut header = String::new();
            loop {
                if stream.read_line(&mut header).await.unwrap() == 0 || header == "\r\n" {
                    break;
                }
                header.clear();
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let scenarios: [(&str, &[&str], &str); 3] = [
        ("/article", &[], "Fixture body"),
        ("/seed", &["--discover"], "Example candidate"),
        ("/seed", &["--discover", "--through", "raw"], "Fixture body"),
    ];
    for (path, options, expected_content) in scenarios {
        let directory = tempfile::tempdir().unwrap();
        let sandbox = directory.path();
        let artifacts = sandbox.join("artifacts");
        write_recommend_history_config(sandbox, &sandbox.join("history.sqlite"));

        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_article-collector"))
            .current_dir(sandbox)
            .env("ARTICLE_COLLECTOR_TEMP_DIR", &artifacts)
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1")
            .env("ACP_AGENT", "unsupported-test-agent")
            .args(["article", "collect", &format!("{origin}{path}")])
            .args(options)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{options:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = std::fs::read(artifacts.join("raw.json")).unwrap();
        let items: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(items[0]["url"], format!("{origin}/article"));
        assert!(
            items[0]["content"]
                .as_str()
                .unwrap()
                .contains(expected_content),
            "{options:?}: {items}"
        );
        assert!(!artifacts.join("translated.md").exists());
    }
    server.abort();
}

#[test]
fn history_clear_keeps_env_scoped_temp_and_output_dirs() {
    let directory = tempfile::tempdir().unwrap();
    let sandbox = directory.path();
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
    let config_path = write_recommend_history_config(sandbox, &history_path);
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
}

#[test]
fn history_clear_with_artifacts_removes_env_scoped_temp_and_output_dirs() {
    let directory = tempfile::tempdir().unwrap();
    let sandbox = directory.path();
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
    let config_path = write_recommend_history_config(sandbox, &history_path);
    insert_seen_recommend_history_item(&history_path);

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .env("ARTICLE_COLLECTOR_TEMP_DIR", &artifact_temp_dir)
        .env("ARTICLE_COLLECTOR_OUTPUT_DIR", &output_dir)
        .args([
            "history",
            "clear",
            "--include-artifacts",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("run article-collector history clear --include-artifacts");

    assert!(
        output.status.success(),
        "expected history clear --include-artifacts to succeed, stderr: {}",
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
        "history clear --include-artifacts should not remove parents"
    );
}
