use std::process::Command;

/// 検証: plugin 管理 command を root CLI から発見できる
/// 理由: optional plugin を必要なユーザーが後付け操作できる入口を提供するため
/// リスク: backend 実装が存在しても CLI から install/configure できない
#[test]
fn root_help_lists_plugin_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .args(["plugin", "--help"])
        .output()
        .expect("run article-collector plugin --help");

    assert!(
        output.status.success(),
        "plugin help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "install",
        "list",
        "update",
        "configure",
        "doctor",
        "uninstall",
    ] {
        assert!(stdout.contains(command), "missing {command}: {stdout}");
    }
}

// Windows resolves LocalAppData through the OS, so overriding HOME cannot isolate it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn plugin_list_writes_installed_plugins_to_stdout() {
    let home = tempfile::tempdir().unwrap();
    let data_home = if cfg!(target_os = "macos") {
        home.path().join("Library/Application Support")
    } else {
        home.path().to_path_buf()
    };
    let plugin_root = data_home.join("article-collector/plugins/example");
    std::fs::create_dir_all(&plugin_root).unwrap();
    std::fs::write(
        plugin_root.join("installed.json"),
        r#"{
          "schema_version": 1,
          "registry_revision": 1,
          "name": "example",
          "version": "1.0.0",
          "executable": "1.0.0/plugin",
          "manifest": {
            "name": "example",
            "version": "1.0.0",
            "protocol": "^1.0",
            "capabilities": ["recommend-source"],
            "sources": [{"name": "example-source", "options_schema": {"type": "object"}}],
            "supports_configuration": false
          }
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .args(["plugin", "list"])
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", data_home)
        .output()
        .expect("run article-collector plugin list");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "plugin list failed: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("example "), "missing plugin: {stdout}");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
}
