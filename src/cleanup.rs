use crate::paths;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ArtifactCleanupSummary {
    pub removed_temp_dir: Option<PathBuf>,
    pub removed_output_dir: Option<PathBuf>,
}

pub fn clear_all_artifacts() -> Result<ArtifactCleanupSummary> {
    let mut summary = ArtifactCleanupSummary::default();

    if let Some(path) = paths::temp_dir_from_env() {
        if remove_env_dir_if_exists(&path, paths::TEMP_DIR_ENV)? {
            summary.removed_temp_dir = Some(path);
        }
    }

    if let Some(path) = paths::output_dir() {
        if remove_env_dir_if_exists(&path, paths::OUTPUT_DIR_ENV)? {
            summary.removed_output_dir = Some(path);
        }
    }

    Ok(summary)
}

fn remove_env_dir_if_exists(path: &Path, env_name: &str) -> Result<bool> {
    reject_unsafe_cleanup_target(path, env_name)?;

    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("failed to remove {env_name} directory {}", path.display())),
    }
}

fn reject_unsafe_cleanup_target(path: &Path, env_name: &str) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("{env_name} is empty");
    }

    if path.parent().is_none() {
        bail!(
            "{env_name} points to a filesystem root and will not be removed: {}",
            path.display()
        );
    }

    Ok(())
}
