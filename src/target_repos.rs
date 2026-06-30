use anyhow::{bail, Context, Result};
use chrono::Local;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const GH_BIN_ENV: &str = "ARTICLE_COLLECTOR_GH_BIN";
const GIT_BIN_ENV: &str = "ARTICLE_COLLECTOR_GIT_BIN";

#[derive(Debug)]
pub struct PreparedTargetRepo {
    pub target_dir: PathBuf,
    pub branch: String,
}

pub fn target_dir_from_env() -> PathBuf {
    std::env::var("TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate::paths::default_target_dir())
}

pub fn prepare_article_branch() -> Result<PreparedTargetRepo> {
    let target_repo = std::env::var("TARGET_REPO").context("TARGET_REPO env var required")?;
    let target_dir = target_dir_from_env();
    let branch = article_branch_name();

    if is_valid_target_repo(&target_dir) {
        run_git(&target_dir, &["checkout", "main"])?;
        run_git(&target_dir, &["pull", "origin", "main"])?;
    } else {
        if target_dir.exists() {
            quarantine_invalid_target_dir(&target_dir)?;
        }
        clone_target_repo(&target_repo, &target_dir)?;
    }

    run_git(&target_dir, &["checkout", "-b", &branch])?;
    Ok(PreparedTargetRepo { target_dir, branch })
}

pub fn create_pr_for_path(path: &Path) -> Result<()> {
    let target_dir = target_dir_from_env();
    let rel_paths = repo_relative_paths_for_inputs(&target_dir, &[path.to_path_buf()])?;
    let rel_path = rel_paths
        .first()
        .context("No paths supplied for PR creation")?;
    let title = format!("collect: {}", rel_path.display());

    let pr_body = format!("## Collected Article\n\n- `{}`", rel_path.to_string_lossy());
    commit_push_and_create_pr(&target_dir, &rel_paths, &title, &title, &pr_body)?;
    Ok(())
}

pub fn create_pr_for_paths(
    paths: &[PathBuf],
    commit_message: &str,
    pr_title: &str,
    pr_body: &str,
) -> Result<()> {
    let target_dir = target_dir_from_env();
    let rel_paths = repo_relative_paths_for_inputs(&target_dir, paths)?;

    commit_push_and_create_pr(&target_dir, &rel_paths, commit_message, pr_title, pr_body)?;
    Ok(())
}

pub fn resolve_repo_path(target_dir: &Path, input: &Path) -> Result<PathBuf> {
    let resolved = if input.is_absolute() {
        normalize_path(input)
    } else {
        normalize_path(&target_dir.join(input))
    };
    let target_dir = normalize_path(target_dir);

    if !resolved.starts_with(&target_dir) {
        bail!(
            "Path {} is outside target repo {}",
            resolved.display(),
            target_dir.display()
        );
    }

    Ok(resolved)
}

pub fn repo_relative_path(target_dir: &Path, path: &Path) -> Result<PathBuf> {
    let target_dir = normalize_path(target_dir);
    let path = normalize_path(path);
    path.strip_prefix(&target_dir)
        .map(Path::to_path_buf)
        .with_context(|| {
            format!(
                "Path {} is outside target repo {}",
                path.display(),
                target_dir.display()
            )
        })
}

fn repo_relative_paths_for_inputs(target_dir: &Path, inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if inputs.is_empty() {
        bail!("No paths supplied for PR creation");
    }

    inputs
        .iter()
        .map(|input| {
            let resolved = resolve_repo_path(target_dir, input)?;
            repo_relative_path(target_dir, &resolved)
        })
        .collect()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn ensure_non_main_branch(target_dir: &Path) -> Result<()> {
    let branch = current_branch(target_dir)?;
    if branch == "main" {
        let new_branch = article_branch_name();
        run_git(target_dir, &["checkout", "-b", &new_branch])?;
    }
    Ok(())
}

fn current_branch(target_dir: &Path) -> Result<String> {
    let output = run_git_output_in_with_timeout(
        target_dir,
        &["branch", "--show-current"],
        command_timeout(),
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn article_branch_name() -> String {
    format!("article/{}", Local::now().format("%Y-%m-%d-%H%M%S"))
}

fn is_valid_target_repo(target_dir: &Path) -> bool {
    if !target_dir.join(".git").exists() {
        return false;
    }

    let Ok(output) = run_git_output_in_with_timeout(
        target_dir,
        &["rev-parse", "--is-inside-work-tree"],
        health_check_timeout(),
    ) else {
        return false;
    };
    if String::from_utf8_lossy(&output.stdout).trim() != "true" {
        return false;
    }

    run_cmd_in_with_timeout(
        target_dir,
        "git",
        &["status", "--porcelain"],
        health_check_timeout(),
    )
    .is_ok()
}

fn clone_target_repo(target_repo: &str, target_dir: &Path) -> Result<()> {
    let parent = target_dir.parent().with_context(|| {
        format!(
            "Target repo path {} does not have a parent directory",
            target_dir.display()
        )
    })?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "Failed to create target repo parent directory {}",
            parent.display()
        )
    })?;

    let staging_dir = clone_staging_path(target_dir);
    let staging_dir_arg = staging_dir.to_string_lossy().to_string();
    let gh = gh_program();
    let clone_result = run_cmd(
        &gh,
        &["repo", "clone", target_repo, staging_dir_arg.as_str()],
    );
    if let Err(error) = clone_result {
        remove_dir_all_if_exists(&staging_dir)?;
        return Err(error).with_context(|| {
            format!(
                "gh repo clone {target_repo} {} failed",
                staging_dir.display()
            )
        });
    }

    if !is_valid_target_repo(&staging_dir) {
        remove_dir_all_if_exists(&staging_dir)?;
        bail!(
            "gh repo clone {target_repo} {} did not create a valid git work tree",
            staging_dir.display()
        );
    }

    fs::rename(&staging_dir, target_dir).with_context(|| {
        format!(
            "Failed to promote cloned target repo from {} to {}",
            staging_dir.display(),
            target_dir.display()
        )
    })?;
    Ok(())
}

fn quarantine_invalid_target_dir(target_dir: &Path) -> Result<PathBuf> {
    let broken_dir = unique_sibling_path(target_dir, "broken");
    fs::rename(target_dir, &broken_dir).with_context(|| {
        format!(
            "Failed to quarantine invalid target repo {} to {}",
            target_dir.display(),
            broken_dir.display()
        )
    })?;
    Ok(broken_dir)
}

fn clone_staging_path(target_dir: &Path) -> PathBuf {
    let parent = target_dir.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let candidate = parent.join(format!(".c{pid}"));
    if !candidate.exists() {
        return candidate;
    }

    for attempt in 1..100 {
        let candidate = parent.join(format!(".c{pid}-{attempt}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    parent.join(format!(".c{pid}-fallback"))
}

fn unique_sibling_path(target_dir: &Path, label: &str) -> PathBuf {
    let parent = target_dir.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target_dir
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| "target-repo".into());
    let stamp = Local::now().format("%Y%m%d%H%M%S%3f");
    let pid = std::process::id();

    for attempt in 0..100 {
        let suffix = if attempt == 0 {
            format!("{label}-{stamp}-{pid}")
        } else {
            format!("{label}-{stamp}-{pid}-{attempt}")
        };
        let candidate = parent.join(format!("{file_name}.{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    parent.join(format!("{file_name}.{label}-{stamp}-{pid}-fallback"))
}

fn remove_dir_all_if_exists(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to remove directory {}", path.display()))
        }
    }
}

fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
    run_git_with_timeout(dir, args, command_timeout())
}

fn run_git_with_timeout(dir: &Path, args: &[&str], timeout: Duration) -> Result<()> {
    run_cmd_in_with_timeout(dir, &git_program(), args, timeout)
}

fn run_git_owned(dir: &Path, args: &[String]) -> Result<()> {
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_cmd_in(dir, "git", &arg_refs)
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    run_cmd_with_timeout(cmd, args, command_timeout())
}

fn run_cmd_in(dir: &Path, cmd: &str, args: &[&str]) -> Result<()> {
    run_cmd_in_with_timeout(dir, cmd, args, command_timeout())
}

fn run_git_output_in_with_timeout(dir: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
    run_output_in_with_timeout(dir, &git_program(), args, timeout)
}

fn run_cmd_with_timeout(cmd: &str, args: &[&str], timeout: Duration) -> Result<()> {
    let mut command = Command::new(cmd);
    command.args(args);
    wait_for_status(command, command_display(cmd, args), timeout)
}

fn run_cmd_in_with_timeout(dir: &Path, cmd: &str, args: &[&str], timeout: Duration) -> Result<()> {
    let mut command = Command::new(cmd);
    command.current_dir(dir).args(args);
    wait_for_status(command, command_display(cmd, args), timeout)
}

fn run_output_in_with_timeout(
    dir: &Path,
    cmd: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output> {
    let mut command = Command::new(cmd);
    command
        .current_dir(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    wait_for_output(command, command_display(cmd, args), timeout)
}

fn wait_for_status(mut command: Command, display: String, timeout: Duration) -> Result<()> {
    let mut child = command
        .spawn()
        .with_context(|| format!("Failed to run command: {display}"))?;
    let started_at = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("Failed to poll command: {display}"))?
        {
            if status.success() {
                return Ok(());
            }
            bail!("{display} failed with {status}");
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{display} timed out after {}", format_duration(timeout));
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_output(mut command: Command, display: String, timeout: Duration) -> Result<Output> {
    let mut child = command
        .spawn()
        .with_context(|| format!("Failed to run command: {display}"))?;
    let started_at = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("Failed to poll command: {display}"))?
        {
            let output = child
                .wait_with_output()
                .with_context(|| format!("Failed to read command output: {display}"))?;
            if status.success() {
                return Ok(output);
            }
            bail!("{display} failed with {status}");
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{display} timed out after {}", format_duration(timeout));
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn command_timeout() -> Duration {
    Duration::from_secs(300)
}

fn health_check_timeout() -> Duration {
    Duration::from_secs(10)
}

fn command_display(cmd: &str, args: &[&str]) -> String {
    if args.is_empty() {
        cmd.to_string()
    } else {
        format!("{cmd} {}", args.join(" "))
    }
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn commit_push_and_create_pr(
    target_dir: &Path,
    rel_paths: &[PathBuf],
    commit_message: &str,
    pr_title: &str,
    pr_body: &str,
) -> Result<()> {
    ensure_non_main_branch(target_dir)?;

    let mut add_args = vec!["add".to_string()];
    add_args.extend(
        rel_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string()),
    );
    run_git_owned(target_dir, &add_args)?;
    run_git(target_dir, &["commit", "-m", commit_message])?;
    let branch = current_branch(target_dir)?;
    push_branch(target_dir, &branch)?;

    run_cmd_in(
        target_dir,
        &gh_program(),
        &["pr", "create", "--title", pr_title, "--body", pr_body],
    )?;

    if std::env::var("AUTO_MERGE").unwrap_or_else(|_| "true".to_string()) == "true" {
        run_cmd_in(target_dir, &gh_program(), &["pr", "merge", "--merge"])?;
    }

    Ok(())
}

fn push_branch(target_dir: &Path, branch: &str) -> Result<()> {
    push_branch_with_timeout(target_dir, branch, command_timeout())
}

fn push_branch_with_timeout(target_dir: &Path, branch: &str, timeout: Duration) -> Result<()> {
    let result = run_git_with_timeout(target_dir, &["push", "-u", "origin", branch], timeout);
    if result.is_ok() {
        return Ok(());
    }

    let error = result.unwrap_err();
    if is_timeout_error(&error) && remote_branch_exists(target_dir, branch) {
        return Ok(());
    }

    Err(error)
}

fn remote_branch_exists(target_dir: &Path, branch: &str) -> bool {
    run_git_with_timeout(
        target_dir,
        &["ls-remote", "--exit-code", "--heads", "origin", branch],
        health_check_timeout(),
    )
    .is_ok()
}

fn is_timeout_error(error: &anyhow::Error) -> bool {
    error.to_string().contains("timed out after")
}

fn gh_program() -> String {
    std::env::var(GH_BIN_ENV).unwrap_or_else(|_| "gh".to_string())
}

fn git_program() -> String {
    std::env::var(GIT_BIN_ENV).unwrap_or_else(|_| "git".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use std::time::Duration;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolves_relative_path_under_target_dir() {
        let target_dir = normalized_temp_path("target-repo-relative");
        let resolved =
            resolve_repo_path(&target_dir, Path::new("articles/web/example.md")).unwrap();

        assert_eq!(resolved, target_dir.join("articles/web/example.md"));
    }

    #[test]
    fn accepts_absolute_path_under_target_dir() {
        let target_dir = normalized_temp_path("target-repo-absolute");
        let input = target_dir.join("articles/web/example.md");
        let resolved = resolve_repo_path(&target_dir, &input).unwrap();

        assert_eq!(resolved, input);
    }

    #[test]
    fn rejects_path_outside_target_dir() {
        let target_dir = normalized_temp_path("target-repo-outside");
        let outside = normalized_temp_path("other-repo").join("example.md");

        assert!(resolve_repo_path(&target_dir, &outside).is_err());
    }

    #[test]
    fn rejects_parent_dir_escape_from_target_dir() {
        let target_dir = normalized_temp_path("target-repo-parent-escape");

        assert!(resolve_repo_path(&target_dir, Path::new("../outside.md")).is_err());
    }

    #[test]
    fn resolves_multiple_pr_paths_to_repo_relative_paths() {
        let target_dir = normalized_temp_path("target-repo-multi-path");
        let absolute = target_dir.join("articles/paper/example.md");

        let relative_paths = repo_relative_paths_for_inputs(
            &target_dir,
            &[PathBuf::from("articles/web/example.md"), absolute.clone()],
        )
        .unwrap();

        assert_eq!(
            relative_paths,
            vec![
                PathBuf::from("articles/web/example.md"),
                PathBuf::from("articles/paper/example.md"),
            ]
        );
    }

    #[test]
    fn rejects_empty_pr_path_list() {
        let target_dir = normalized_temp_path("target-repo-empty-paths");

        let error = repo_relative_paths_for_inputs(&target_dir, &[]).unwrap_err();

        assert_eq!(error.to_string(), "No paths supplied for PR creation");
    }

    #[test]
    fn invalid_git_marker_is_not_reused_as_target_repo() {
        let target_dir = normalized_temp_path("target-repo-invalid-marker");
        let _ = fs::remove_dir_all(&target_dir);
        fs::create_dir_all(target_dir.join(".git")).unwrap();

        assert!(!is_valid_target_repo(&target_dir));

        let _ = fs::remove_dir_all(target_dir);
    }

    #[test]
    fn clone_staging_path_uses_short_fixed_basename() {
        let target_dir = normalized_temp_path("target-repo-short-clone");

        let staging_dir = clone_staging_path(&target_dir);

        assert_eq!(staging_dir.parent(), target_dir.parent());
        assert_eq!(
            staging_dir.file_name().unwrap(),
            format!(".c{}", std::process::id()).as_str()
        );
        assert!(
            staging_dir.to_string_lossy().len() < target_dir.to_string_lossy().len(),
            "clone staging path should be shorter than final target path: staging={}, target={}",
            staging_dir.display(),
            target_dir.display()
        );
    }

    #[test]
    fn clone_staging_path_keeps_short_name_when_primary_staging_exists() {
        let sandbox = normalized_temp_path("target-repo-short-clone-fallback");
        let target_dir = sandbox.join("target-repo");
        let primary_staging_dir = sandbox.join(format!(".c{}", std::process::id()));
        let _ = fs::remove_dir_all(&sandbox);
        fs::create_dir_all(&primary_staging_dir).unwrap();

        let staging_dir = clone_staging_path(&target_dir);

        assert_eq!(staging_dir.parent(), target_dir.parent());
        assert_eq!(
            staging_dir.file_name().unwrap(),
            format!(".c{}-1", std::process::id()).as_str()
        );
        assert!(
            staging_dir.to_string_lossy().len() < target_dir.to_string_lossy().len(),
            "fallback clone staging path should stay short: staging={}, target={}",
            staging_dir.display(),
            target_dir.display()
        );

        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn clone_staging_path_keeps_observed_windows_pdf_path_under_max_path() {
        const WINDOWS_MAX_PATH: usize = 260;
        const RUN_DIR: &str = r"D:\lifelog\reports\article-collector\20260629T070027Z-news";
        const PROBLEM_RELATIVE_PATH: &str = r"1_zettels\career\background\publications\Development of a hyper CLS data-based robotic interface for automation of production-line tasks using an articulated robot arm.pdf";

        let target_path = format!(r"{RUN_DIR}\target-repo\{PROBLEM_RELATIVE_PATH}");
        let old_staging_path =
            format!(r"{RUN_DIR}\target-repo.clone-20260629101116123-12345\{PROBLEM_RELATIVE_PATH}");
        let short_staging_path = format!(r"{RUN_DIR}\.c12345\{PROBLEM_RELATIVE_PATH}");
        let short_fallback_path = format!(r"{RUN_DIR}\.c12345-1\{PROBLEM_RELATIVE_PATH}");

        assert_eq!(target_path.len(), 242);
        assert_eq!(old_staging_path.len(), 272);
        assert_eq!(short_staging_path.len(), 238);
        assert_eq!(short_fallback_path.len(), 240);
        assert!(target_path.len() < WINDOWS_MAX_PATH);
        assert!(old_staging_path.len() > WINDOWS_MAX_PATH);
        assert!(short_staging_path.len() < WINDOWS_MAX_PATH);
        assert!(short_fallback_path.len() < WINDOWS_MAX_PATH);
    }

    #[test]
    fn failed_clone_cleans_staging_dir_and_quarantines_invalid_target() {
        let _guard = ENV_LOCK.lock().unwrap();
        let sandbox = normalized_temp_path("target-repo-failed-clone");
        let target_dir = sandbox.join("target-repo");
        let bin_dir = sandbox.join("bin");
        let _ = fs::remove_dir_all(&sandbox);
        fs::create_dir_all(target_dir.join(".git")).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_gh = write_fake_gh_clone_failure(&bin_dir);

        let previous_path = std::env::var_os("PATH");
        let previous_gh_bin = std::env::var_os(GH_BIN_ENV);
        std::env::set_var("PATH", prepend_path(&bin_dir, previous_path.as_ref()));
        std::env::set_var(GH_BIN_ENV, &fake_gh);
        std::env::set_var("TARGET_REPO", "rurusasu/lifelog");
        std::env::set_var("TARGET_DIR", &target_dir);

        let error = prepare_article_branch().unwrap_err();

        assert!(
            error.to_string().contains("gh repo clone"),
            "unexpected error: {error:#}"
        );
        assert!(
            !target_dir.exists(),
            "invalid target dir should be moved away before clone retry"
        );
        let entries = fs::read_dir(&sandbox)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(
            entries
                .iter()
                .any(|name| name.starts_with("target-repo.broken-")),
            "broken target dir should be quarantined, entries: {entries:?}"
        );
        assert!(
            entries.iter().all(|name| !name.contains(".clone-")),
            "failed clone staging dirs should be cleaned up, entries: {entries:?}"
        );

        restore_env("PATH", previous_path);
        restore_env(GH_BIN_ENV, previous_gh_bin);
        std::env::remove_var("TARGET_REPO");
        std::env::remove_var("TARGET_DIR");
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn external_command_times_out_instead_of_hanging() {
        let result = if cfg!(windows) {
            run_cmd_with_timeout(
                "powershell",
                &["-NoProfile", "-Command", "Start-Sleep -Seconds 5"],
                Duration::from_millis(50),
            )
        } else {
            run_cmd_with_timeout("sh", &["-c", "sleep 5"], Duration::from_millis(50))
        };

        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("timed out"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn push_timeout_is_recovered_when_remote_branch_exists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let sandbox = normalized_temp_path("target-repo-push-timeout-recovered");
        let target_dir = sandbox.join("target-repo");
        let bin_dir = sandbox.join("bin");
        let _ = fs::remove_dir_all(&sandbox);
        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_git = write_fake_git_push_timeout(&bin_dir);

        let previous_git_bin = std::env::var_os(GIT_BIN_ENV);
        std::env::set_var(GIT_BIN_ENV, &fake_git);

        let result =
            push_branch_with_timeout(&target_dir, "article/recovered", Duration::from_millis(50));

        assert!(
            result.is_ok(),
            "push timeout should be recovered: {result:#?}"
        );

        restore_env(GIT_BIN_ENV, previous_git_bin);
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn push_timeout_remains_error_when_remote_branch_is_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let sandbox = normalized_temp_path("target-repo-push-timeout-missing");
        let target_dir = sandbox.join("target-repo");
        let bin_dir = sandbox.join("bin");
        let _ = fs::remove_dir_all(&sandbox);
        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_git = write_fake_git_push_timeout(&bin_dir);

        let previous_git_bin = std::env::var_os(GIT_BIN_ENV);
        std::env::set_var(GIT_BIN_ENV, &fake_git);

        let error =
            push_branch_with_timeout(&target_dir, "article/missing", Duration::from_millis(50))
                .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("push -u origin article/missing timed out after 50ms"),
            "unexpected error: {error:#}"
        );

        restore_env(GIT_BIN_ENV, previous_git_bin);
        let _ = fs::remove_dir_all(sandbox);
    }

    fn normalized_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("article-collector-{name}-{}", std::process::id()))
    }

    fn write_fake_gh_clone_failure(bin_dir: &Path) -> PathBuf {
        if cfg!(windows) {
            let path = bin_dir.join("gh.cmd");
            fs::write(
                &path,
                "@echo off\r\nmkdir \"%4\\.git\" 2>nul\r\nexit /b 1\r\n",
            )
            .unwrap();
            path
        } else {
            let path = bin_dir.join("gh");
            fs::write(&path, "#!/usr/bin/env sh\nmkdir -p \"$4/.git\"\nexit 1\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fs::metadata(&path).unwrap().permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&path, permissions).unwrap();
            }
            path
        }
    }

    fn write_fake_git_push_timeout(bin_dir: &Path) -> PathBuf {
        if cfg!(windows) {
            let path = bin_dir.join("git.cmd");
            fs::write(
                &path,
                concat!(
                    "@echo off\r\n",
                    "if \"%1\"==\"push\" (\r\n",
                    "  powershell -NoProfile -Command \"Start-Sleep -Milliseconds 500\"\r\n",
                    "  exit /b 0\r\n",
                    ")\r\n",
                    "if \"%1\"==\"ls-remote\" (\r\n",
                    "  if \"%5\"==\"article/recovered\" exit /b 0\r\n",
                    "  exit /b 2\r\n",
                    ")\r\n",
                    "exit /b 1\r\n",
                ),
            )
            .unwrap();
            path
        } else {
            let path = bin_dir.join("git");
            fs::write(
                &path,
                concat!(
                    "#!/usr/bin/env sh\n",
                    "if [ \"$1\" = \"push\" ]; then\n",
                    "  sleep 1\n",
                    "  exit 0\n",
                    "fi\n",
                    "if [ \"$1\" = \"ls-remote\" ]; then\n",
                    "  if [ \"$5\" = \"article/recovered\" ]; then exit 0; fi\n",
                    "  exit 2\n",
                    "fi\n",
                    "exit 1\n",
                ),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fs::metadata(&path).unwrap().permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&path, permissions).unwrap();
            }
            path
        }
    }

    fn prepend_path(
        bin_dir: &Path,
        previous_path: Option<&std::ffi::OsString>,
    ) -> std::ffi::OsString {
        let mut paths = vec![bin_dir.to_path_buf()];
        if let Some(previous_path) = previous_path {
            paths.extend(std::env::split_paths(previous_path));
        }
        std::env::join_paths(paths).unwrap()
    }

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}
