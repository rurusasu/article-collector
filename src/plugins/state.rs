use crate::plugins::paths::{validate_plugin_name, PluginPaths};
use crate::plugins::types::InstalledPlugin;
use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Component;
use tempfile::NamedTempFile;

#[derive(Debug, Clone)]
pub struct InstalledPluginStore {
    paths: PluginPaths,
}

impl InstalledPluginStore {
    pub fn new(paths: PluginPaths) -> Self {
        Self { paths }
    }

    pub(crate) fn paths(&self) -> &PluginPaths {
        &self.paths
    }

    pub fn list(&self) -> Result<Vec<InstalledPlugin>> {
        let root = self.paths.plugins_root();
        self.paths.ensure_safe_plugin_path(&root)?;
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to list plugins in {}", root.display()))
            }
        };

        let mut names = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Plugin directory name is not UTF-8"))?;
            validate_plugin_name(&name)?;
            names.push(name);
        }
        names.sort();

        names
            .into_iter()
            .filter_map(|name| match self.get(&name) {
                Ok(Some(plugin)) => Some(Ok(plugin)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub fn get(&self, name: &str) -> Result<Option<InstalledPlugin>> {
        let plugin_root = self.paths.plugin_root(name)?;
        let installed_path = plugin_root.join("installed.json");
        self.paths.ensure_safe_plugin_path(&installed_path)?;
        let bytes = match fs::read(&installed_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Failed to read plugin state {}", installed_path.display())
                })
            }
        };
        let installed: InstalledPlugin = serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid plugin state {}", installed_path.display()))?;
        if installed.name != name {
            bail!(
                "Plugin state name '{}' does not match directory '{name}'",
                installed.name
            );
        }
        Ok(Some(installed))
    }

    pub fn resolve_runtime_plugin(&self, mut plugin: InstalledPlugin) -> Result<InstalledPlugin> {
        if plugin.executable.is_absolute()
            || plugin
                .executable
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            bail!(
                "Plugin '{}' has an unsafe executable path '{}'",
                plugin.name,
                plugin.executable.display()
            );
        }
        let executable = self
            .paths
            .plugin_root(&plugin.name)?
            .join(&plugin.executable);
        self.paths.ensure_safe_plugin_path(&executable)?;
        let metadata = fs::metadata(&executable).with_context(|| {
            format!(
                "Plugin '{}' executable does not exist: {}",
                plugin.name,
                executable.display()
            )
        })?;
        if !metadata.is_file() {
            bail!(
                "Plugin '{}' executable is not a file: {}",
                plugin.name,
                executable.display()
            );
        }
        plugin.executable = executable;
        Ok(plugin)
    }

    pub fn activate(&self, installed: &InstalledPlugin) -> Result<()> {
        self.activate_with_writer(installed, |file| {
            serde_json::to_writer_pretty(&mut *file, installed).map_err(io::Error::other)?;
            file.write_all(b"\n")
        })
    }

    fn activate_with_writer<F>(&self, installed: &InstalledPlugin, writer: F) -> Result<()>
    where
        F: FnOnce(&mut File) -> io::Result<()>,
    {
        validate_plugin_name(&installed.name)?;
        let plugin_root = self.paths.plugin_root(&installed.name)?;
        fs::create_dir_all(&plugin_root)
            .with_context(|| format!("Failed to create {}", plugin_root.display()))?;
        self.paths.ensure_safe_plugin_path(&plugin_root)?;

        let mut temp = NamedTempFile::new_in(&plugin_root)
            .with_context(|| format!("Failed to stage state in {}", plugin_root.display()))?;
        writer(temp.as_file_mut()).context("Failed to write staged plugin state")?;
        temp.as_file()
            .sync_all()
            .context("Failed to sync staged plugin state")?;
        let installed_path = plugin_root.join("installed.json");
        temp.persist(&installed_path)
            .map_err(|error| error.error)
            .with_context(|| format!("Failed to activate {}", installed_path.display()))?;
        sync_parent(&plugin_root)?;
        Ok(())
    }

    pub fn uninstall(&self, name: &str) -> Result<bool> {
        let plugin_root = self.paths.plugin_root(name)?;
        if !plugin_root.exists() {
            return Ok(false);
        }
        self.paths.ensure_safe_plugin_path(&plugin_root)?;
        fs::remove_dir_all(&plugin_root)
            .with_context(|| format!("Failed to remove plugin {}", plugin_root.display()))?;
        sync_parent(&self.paths.plugins_root())?;
        Ok(true)
    }
}

fn sync_parent(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)
            .with_context(|| format!("Failed to open directory {}", path.display()))?
            .sync_all()
            .with_context(|| format!("Failed to sync directory {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    fn fixture_installed(name: &str, version: &str) -> InstalledPlugin {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "registry_revision": 7,
            "name": name,
            "version": version,
            "executable": format!("{version}/example"),
            "manifest": {
                "name": name,
                "version": version,
                "protocol": "^1.0",
                "capabilities": ["recommend-source"],
                "sources": [{
                    "name": "example",
                    "aliases": [],
                    "options_schema": {"type": "object", "additionalProperties": false}
                }],
                "supports_configuration": false,
                "environment": []
            }
        }))
        .unwrap()
    }

    fn fixture_store(root: &std::path::Path) -> InstalledPluginStore {
        InstalledPluginStore::new(PluginPaths::from_data_root(root.to_path_buf()))
    }

    /// 検証: installed.json書き込み失敗時も以前のactive versionを保持する
    /// 理由: update中断で正常なpluginを利用不能にしないため
    /// リスク: partial stateがactiveになりrecommend全体が失敗する
    #[test]
    fn failed_activation_keeps_previous_installed_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = fixture_store(temp.path());
        store
            .activate(&fixture_installed("example", "1.0.0"))
            .unwrap();

        let result = store.activate_with_writer(&fixture_installed("example", "1.1.0"), |_| {
            Err(std::io::Error::other("injected failure"))
        });

        assert!(result.is_err());
        assert_eq!(
            store.get("example").unwrap().unwrap().version,
            Version::parse("1.0.0").unwrap()
        );
    }

    /// 検証: listはplugin名順で、uninstallは指定pluginだけを除去する
    /// 理由: CLI出力を決定的にし、隣接pluginを誤削除しないため
    /// リスク: 削除scopeが広がり別pluginの設定を失う
    #[test]
    fn lists_deterministically_and_uninstalls_exact_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let store = fixture_store(temp.path());
        store.activate(&fixture_installed("zeta", "1.0.0")).unwrap();
        store
            .activate(&fixture_installed("alpha", "2.0.0"))
            .unwrap();

        let names = store
            .list()
            .unwrap()
            .into_iter()
            .map(|plugin| plugin.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["alpha", "zeta"]);

        assert!(store.uninstall("alpha").unwrap());
        assert!(store.get("alpha").unwrap().is_none());
        assert!(store.get("zeta").unwrap().is_some());
        assert!(!store.uninstall("alpha").unwrap());
    }
}
