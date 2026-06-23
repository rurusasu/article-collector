use anyhow::{bail, Context, Result};
use chrono::Local;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

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

    if target_dir.join(".git").exists() {
        run_git(&target_dir, &["checkout", "main"])?;
        run_git(&target_dir, &["pull", "origin", "main"])?;
    } else {
        let target_dir_arg = target_dir.to_string_lossy().to_string();
        run_cmd("gh", &["repo", "clone", &target_repo, &target_dir_arg])?;
    }

    run_git(&target_dir, &["checkout", "-b", &branch])?;
    Ok(PreparedTargetRepo { target_dir, branch })
}

pub fn create_pr_for_path(path: &Path) -> Result<()> {
    let target_dir = target_dir_from_env();
    let resolved = resolve_repo_path(&target_dir, path)?;
    let rel_path = repo_relative_path(&target_dir, &resolved)?;
    ensure_non_main_branch(&target_dir)?;

    let rel_path_arg = rel_path.to_string_lossy().to_string();
    run_git(&target_dir, &["add", &rel_path_arg])?;
    run_git(
        &target_dir,
        &["commit", "-m", &format!("collect: {}", rel_path.display())],
    )?;
    let branch = current_branch(&target_dir)?;
    run_git(&target_dir, &["push", "-u", "origin", &branch])?;

    let pr_body = format!("## Collected Article\n\n- `{}`", rel_path.to_string_lossy());
    run_cmd_in(
        &target_dir,
        "gh",
        &[
            "pr",
            "create",
            "--title",
            &format!("collect: {}", rel_path.display()),
            "--body",
            &pr_body,
        ],
    )?;

    if std::env::var("AUTO_MERGE").unwrap_or_else(|_| "true".to_string()) == "true" {
        run_cmd_in(&target_dir, "gh", &["pr", "merge", "--merge"])?;
    }

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
    let output = Command::new("git")
        .current_dir(target_dir)
        .args(["branch", "--show-current"])
        .output()
        .context("git branch --show-current failed")?;
    if !output.status.success() {
        bail!("git branch --show-current failed with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn article_branch_name() -> String {
    format!("article/{}", Local::now().format("%Y-%m-%d-%H%M%S"))
}

fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
    run_cmd_in(dir, "git", args)
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).args(args).status()?;
    if !status.success() {
        bail!("{cmd} {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn run_cmd_in(dir: &Path, cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).current_dir(dir).args(args).status()?;
    if !status.success() {
        bail!("{cmd} {} failed with {status}", args.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn normalized_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("article-collector-{name}-{}", std::process::id()))
    }
}
