use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPaths {
    pub data_root: PathBuf,
}

impl PluginPaths {
    pub fn from_data_root(data_root: PathBuf) -> Self {
        Self { data_root }
    }

    pub fn system() -> Result<Self> {
        let data_root = crate::paths::plugin_data_root()
            .context("Unable to resolve the operating system user data directory")?;
        Ok(Self::from_data_root(data_root))
    }

    pub fn registry_root(&self) -> PathBuf {
        self.data_root.join("registry")
    }

    pub fn plugins_root(&self) -> PathBuf {
        self.data_root.join("plugins")
    }

    pub fn plugin_root(&self, name: &str) -> Result<PathBuf> {
        validate_plugin_name(name)?;
        let path = self.plugins_root().join(name);
        self.ensure_safe_plugin_path(&path)?;
        Ok(path)
    }

    pub fn ensure_safe_plugin_path(&self, target: &Path) -> Result<()> {
        if !target.starts_with(&self.data_root) {
            bail!(
                "Plugin path '{}' escapes data root '{}'",
                target.display(),
                self.data_root.display()
            );
        }

        reject_symlinks_on_existing_path(&self.data_root)?;
        let relative = target
            .strip_prefix(&self.data_root)
            .expect("starts_with checked above");
        let mut current = self.data_root.clone();
        for component in relative.components() {
            if !matches!(component, Component::Normal(_)) {
                bail!("Plugin path contains an unsafe component");
            }
            current.push(component);
            reject_symlink_if_present(&current)?;
        }
        Ok(())
    }
}

pub fn current_plugin_target() -> Result<&'static str> {
    plugin_target(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn plugin_target(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        _ => bail!("Unsupported plugin target: {os}/{arch}"),
    }
}

pub fn plugin_target_from_triple(target: &str) -> Result<&str> {
    match target {
        "x86_64-unknown-linux-gnu"
        | "aarch64-unknown-linux-gnu"
        | "x86_64-pc-windows-msvc"
        | "x86_64-apple-darwin"
        | "aarch64-apple-darwin" => Ok(target),
        _ => bail!("Unsupported plugin target triple: {target}"),
    }
}

pub fn validate_plugin_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    let valid = (1..=64).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    if !valid {
        bail!("Invalid plugin name '{name}': expected [a-z0-9][a-z0-9-]{{0,63}}");
    }
    Ok(())
}

fn reject_symlinks_on_existing_path(path: &Path) -> Result<()> {
    if path.exists() {
        reject_symlink_if_present(path)?;
    }
    Ok(())
}

fn reject_symlink_if_present(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to inspect plugin path {}", path.display()))
        }
    };
    if metadata.file_type().is_symlink() || is_windows_reparse_point(&metadata) {
        bail!(
            "Plugin path contains a symbolic link or reparse point: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 検証: release が提供する5 targetだけをOS/archから解決する
    /// 理由: 署名済みregistryの別platform artifactを誤実行しないため
    /// リスク: 非対応targetを選択し、install後のprobeで壊れる
    #[test]
    fn maps_supported_os_arch_pairs_to_release_targets() {
        let cases = [
            ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
            ("linux", "aarch64", "aarch64-unknown-linux-gnu"),
            ("windows", "x86_64", "x86_64-pc-windows-msvc"),
            ("macos", "x86_64", "x86_64-apple-darwin"),
            ("macos", "aarch64", "aarch64-apple-darwin"),
        ];
        for (os, arch, expected) in cases {
            assert_eq!(plugin_target(os, arch).unwrap(), expected);
        }
        assert!(plugin_target("windows", "aarch64").is_err());
    }

    /// 検証: plugin名は単一の安全なpath segmentに制限する
    /// 理由: uninstall/install対象がplugin root外へ逸脱しないため
    /// リスク: path traversalでユーザーファイルを上書き・削除する
    #[test]
    fn rejects_plugin_names_that_are_not_safe_path_segments() {
        for name in ["", "../example", "Example", "a/b", "-example", "a_b"] {
            assert!(
                validate_plugin_name(name).is_err(),
                "{name} must be rejected"
            );
        }
        assert!(validate_plugin_name("x").is_ok());
        assert!(validate_plugin_name("1password").is_ok());
        assert!(validate_plugin_name("example-plugin").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_inside_plugin_path() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let paths = PluginPaths::from_data_root(temp.path().to_path_buf());
        std::fs::create_dir_all(paths.plugins_root()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), paths.plugins_root().join("example")).unwrap();

        let error = paths.plugin_root("example").unwrap_err();
        assert!(error.to_string().contains("symbolic link"));
    }
}
