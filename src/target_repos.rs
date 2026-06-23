use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct PreparedTargetRepo {
    pub target_dir: PathBuf,
    pub branch: String,
}

#[allow(dead_code)]
pub fn target_dir_from_env() -> PathBuf {
    std::env::var("TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate::paths::default_target_dir())
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        normalized.push(component.as_os_str());
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_path_under_target_dir() {
        let target_dir = normalized_temp_path("target-repo-relative");
        let resolved = resolve_repo_path(&target_dir, Path::new("articles/web/example.md")).unwrap();

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

    fn normalized_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "article-collector-{name}-{}",
            std::process::id()
        ))
    }
}
