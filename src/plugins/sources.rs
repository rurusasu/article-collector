use crate::plugins::runtime::PluginRuntime;
use crate::plugins::state::InstalledPluginStore;
use crate::plugins::types::{CollectRequest, InstalledPlugin, ManifestSource, PluginCandidate};
use crate::sites;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Component;

#[derive(Debug, Clone)]
pub struct PluginSource {
    pub name: String,
    pub plugin: InstalledPlugin,
    options_schema: Value,
}

impl PluginSource {
    pub fn validate_options(&self, options: Option<&toml::Table>) -> Result<Value> {
        let options = serde_json::to_value(options.cloned().unwrap_or_default())
            .context("Failed to convert plugin source options to JSON")?;
        let validator = jsonschema::validator_for(&self.options_schema)
            .with_context(|| format!("Invalid options schema for plugin source '{}'", self.name))?;
        if let Err(error) = validator.validate(&options) {
            bail!("Invalid options for plugin source '{}': {error}", self.name);
        }
        Ok(options)
    }
}

#[async_trait]
pub trait PluginSourceCollector: Send + Sync {
    async fn collect(
        &self,
        source: &PluginSource,
        limit: usize,
        options: Value,
    ) -> Result<Vec<PluginCandidate>>;
}

#[async_trait]
impl PluginSourceCollector for PluginRuntime {
    async fn collect(
        &self,
        source: &PluginSource,
        limit: usize,
        options: Value,
    ) -> Result<Vec<PluginCandidate>> {
        let options = options
            .as_object()
            .cloned()
            .context("Validated plugin source options must be a JSON object")?;
        PluginRuntime::collect(
            self,
            &source.plugin,
            CollectRequest {
                source: source.name.clone(),
                limit,
                options,
            },
        )
        .await
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginSourceCatalog {
    sources: BTreeMap<String, PluginSource>,
    names: BTreeMap<String, String>,
}

impl PluginSourceCatalog {
    pub fn load(store: &InstalledPluginStore) -> Result<Self> {
        let mut catalog = Self::default();
        for mut plugin in store.list()? {
            let executable = resolve_executable(store, &plugin)?;
            plugin.executable = executable;

            for source in plugin.manifest.sources.clone() {
                catalog.insert_source(&plugin, source)?;
            }
        }
        Ok(catalog)
    }

    pub fn resolve(&self, name: &str) -> Option<&PluginSource> {
        let canonical = self.names.get(&name.to_ascii_lowercase())?;
        self.sources.get(canonical)
    }

    pub fn recommendable_sources(&self) -> Vec<String> {
        self.sources.keys().cloned().collect()
    }

    fn insert_source(&mut self, plugin: &InstalledPlugin, source: ManifestSource) -> Result<()> {
        let canonical = source.name.to_ascii_lowercase();
        let mut claimed_names = Vec::with_capacity(source.aliases.len() + 1);
        claimed_names.push(canonical.clone());
        claimed_names.extend(
            source
                .aliases
                .iter()
                .map(|alias| alias.to_ascii_lowercase()),
        );

        for name in &claimed_names {
            if sites::site_by_name(name).is_some() {
                bail!(
                    "Plugin '{}' source name '{}' conflicts with a built-in source",
                    plugin.name,
                    name
                );
            }
            if let Some(existing) = self.names.get(name) {
                bail!(
                    "Plugin '{}' source name '{}' conflicts with plugin source '{}'",
                    plugin.name,
                    name,
                    existing
                );
            }
        }

        let plugin_source = PluginSource {
            name: canonical.clone(),
            plugin: plugin.clone(),
            options_schema: source.options_schema,
        };
        for name in claimed_names {
            self.names.insert(name, canonical.clone());
        }
        self.sources.insert(canonical, plugin_source);
        Ok(())
    }
}

fn resolve_executable(
    store: &InstalledPluginStore,
    plugin: &InstalledPlugin,
) -> Result<std::path::PathBuf> {
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

    let executable = store
        .paths()
        .plugin_root(&plugin.name)?
        .join(&plugin.executable);
    store.paths().ensure_safe_plugin_path(&executable)?;
    let metadata = std::fs::metadata(&executable).with_context(|| {
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
    Ok(executable)
}

pub fn candidate_to_recommendation(
    source_name: &str,
    rank: usize,
    candidate: PluginCandidate,
) -> Value {
    let mut item = candidate.metadata;
    item.insert("title".to_string(), json!(candidate.title));
    item.insert("url".to_string(), json!(candidate.url.as_str()));
    item.insert("content".to_string(), json!(candidate.content));
    item.insert("source".to_string(), json!(source_name));
    item.insert("site".to_string(), json!(source_name));
    item.insert("rank".to_string(), json!(rank));
    Value::Object(item)
}

pub async fn collect_plugin_source<C>(
    collector: &C,
    source: &PluginSource,
    limit: usize,
    options: Option<&toml::Table>,
) -> Result<Vec<Value>>
where
    C: PluginSourceCollector + ?Sized,
{
    let options = source.validate_options(options)?;
    let candidates = collector
        .collect(source, limit, options)
        .await
        .with_context(|| {
            format!(
                "Plugin '{}' failed to collect source '{}'",
                source.plugin.name, source.name
            )
        })?;
    if candidates.len() > limit {
        bail!(
            "Plugin '{}' returned {} candidates for limit {limit}",
            source.plugin.name,
            candidates.len()
        );
    }
    Ok(candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| candidate_to_recommendation(&source.name, index + 1, candidate))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::paths::PluginPaths;
    use crate::plugins::state::InstalledPluginStore;
    use crate::plugins::types::InstalledPlugin;
    use serde_json::json;

    fn installed(plugin: &str, source: &str, aliases: &[&str]) -> InstalledPlugin {
        serde_json::from_value(json!({
            "schema_version": 1,
            "registry_revision": 1,
            "name": plugin,
            "version": "1.0.0",
            "executable": "1.0.0/plugin",
            "manifest": {
                "name": plugin,
                "version": "1.0.0",
                "protocol": "^1.0",
                "capabilities": ["recommend-source"],
                "sources": [{
                    "name": source,
                    "aliases": aliases,
                    "options_schema": {
                        "type": "object",
                        "properties": {
                            "mode": {"enum": ["latest", "popular"]}
                        },
                        "required": ["mode"],
                        "additionalProperties": false
                    }
                }],
                "supports_configuration": false,
                "environment": []
            }
        }))
        .unwrap()
    }

    fn store_with(root: &std::path::Path, plugins: &[InstalledPlugin]) -> InstalledPluginStore {
        let paths = PluginPaths::from_data_root(root.to_path_buf());
        let store = InstalledPluginStore::new(paths.clone());
        for plugin in plugins {
            let executable = paths
                .plugin_root(&plugin.name)
                .unwrap()
                .join(&plugin.executable);
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(executable, b"fixture").unwrap();
            store.activate(plugin).unwrap();
        }
        store
    }

    /// 検証: canonical sourceとaliasが同じinstalled pluginへ解決される
    /// 理由: recommend target解決時にplugin processを起動せずmanifestだけを使うため
    /// リスク: alias指定で別sourceまたはunknown扱いになる
    #[test]
    fn resolves_installed_source_and_alias_without_launching_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(
            temp.path(),
            &[installed("example-plugin", "example", &["ex"])],
        );

        let catalog = PluginSourceCatalog::load(&store).unwrap();

        assert_eq!(catalog.resolve("example").unwrap().name, "example");
        assert_eq!(catalog.resolve("ex").unwrap().name, "example");
        assert_eq!(catalog.recommendable_sources(), vec!["example"]);
    }

    /// 検証: built-in名・aliasとの衝突はcatalog構築時に拒否する
    /// 理由: source ownershipを暗黙に上書きしないため
    /// リスク: install順で実行backendが変化する
    #[test]
    fn rejects_collision_with_built_in_source_namespace() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(
            temp.path(),
            &[installed("example-plugin", "hackernews", &[])],
        );

        let error = PluginSourceCatalog::load(&store).unwrap_err();

        assert!(error.to_string().contains("hackernews"));
        assert!(error.to_string().contains("built-in"));
    }

    /// 検証: plugin同士のcanonical/alias衝突も拒否する
    /// 理由:一意なsource ownerを保証するため
    /// リスク: filesystem列挙順でownerが変化する
    #[test]
    fn rejects_collision_between_installed_plugins() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(
            temp.path(),
            &[
                installed("alpha-plugin", "alpha", &["shared"]),
                installed("beta-plugin", "beta", &["shared"]),
            ],
        );

        let error = PluginSourceCatalog::load(&store).unwrap_err();

        assert!(error.to_string().contains("shared"));
        assert!(error.to_string().contains("conflict"));
    }

    /// 検証: source optionsを署名manifestのJSON Schemaで検証する
    /// 理由: plugin起動前に不正設定を明確なconfig errorとして返すため
    /// リスク: pluginごとに異なる暗黙defaultやruntime failureが起きる
    #[test]
    fn validates_toml_options_against_source_schema() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(temp.path(), &[installed("example-plugin", "example", &[])]);
        let catalog = PluginSourceCatalog::load(&store).unwrap();
        let source = catalog.resolve("example").unwrap();

        let valid: toml::Table = toml::from_str("mode = \"latest\"").unwrap();
        assert_eq!(
            source.validate_options(Some(&valid)).unwrap(),
            json!({"mode": "latest"})
        );

        let invalid: toml::Table = toml::from_str("mode = \"unknown\"").unwrap();
        assert!(source
            .validate_options(Some(&invalid))
            .unwrap_err()
            .to_string()
            .contains("options"));
    }

    /// 検証: plugin metadataを保ちつつsource/site/rankはcoreが設定する
    /// 理由:既存history/save pipelineのcandidate contractへ正規化するため
    /// リスク: attribution欠落またはpluginによるrank偽装が起きる
    #[test]
    fn converts_candidate_to_existing_recommendation_shape() {
        let candidate = serde_json::from_value(json!({
            "title": "Example",
            "url": "https://example.com/1",
            "content": "Body",
            "metadata": {"author": "Alice"}
        }))
        .unwrap();

        let item = candidate_to_recommendation("example", 2, candidate);

        assert_eq!(item["source"], "example");
        assert_eq!(item["site"], "example");
        assert_eq!(item["rank"], 2);
        assert_eq!(item["author"], "Alice");
        assert_eq!(item["url"], "https://example.com/1");
    }
}
