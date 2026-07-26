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
