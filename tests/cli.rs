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
