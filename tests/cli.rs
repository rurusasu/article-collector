use std::process::Command;

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
