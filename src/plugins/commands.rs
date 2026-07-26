use crate::plugins::installer::{InstallOutcome, PluginInstaller, PluginProbe};
use crate::plugins::paths::{current_plugin_target, validate_plugin_name, PluginPaths};
use crate::plugins::registry::{resolve_plugin, RegistryClient};
use crate::plugins::runtime::PluginRuntime;
use crate::plugins::state::InstalledPluginStore;
use crate::plugins::types::{InstalledPlugin, PluginDescriptor, RegistryIndex};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Subcommand;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{self, Write};
use std::str::FromStr;
use tempfile::NamedTempFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReference {
    pub name: String,
    pub version: Option<Version>,
}

impl FromStr for PluginReference {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (name, version) = match value.split_once('@') {
            Some((name, version)) if !version.is_empty() && !version.contains('@') => (
                name,
                Some(
                    Version::parse(version)
                        .map_err(|error| format!("Invalid plugin version '{version}': {error}"))?,
                ),
            ),
            Some(_) => return Err(format!("Invalid plugin reference '{value}'")),
            None => (value, None),
        };
        validate_plugin_name(name).map_err(|error| error.to_string())?;
        Ok(Self {
            name: name.to_string(),
            version,
        })
    }
}

#[derive(Debug, Subcommand)]
pub enum PluginCommands {
    /// Install a plugin from the official signed registry
    Install {
        /// Plugin name, optionally pinned as name@version
        plugin: PluginReference,
    },
    /// List installed plugins
    List,
    /// Update an installed plugin to the latest compatible version
    Update {
        /// Installed plugin name
        name: String,
    },
    /// Run a plugin's interactive configuration
    Configure {
        /// Installed plugin name
        name: String,
    },
    /// Validate an installed plugin and report stale staging directories
    Doctor {
        /// Installed plugin name
        name: String,
    },
    /// Uninstall an exact plugin
    Uninstall {
        /// Installed plugin name
        name: String,
    },
}

#[async_trait]
pub trait PluginRegistry: Send + Sync {
    async fn fetch(
        &self,
        now: DateTime<Utc>,
        cached_revision: Option<u64>,
    ) -> Result<RegistryIndex>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OfficialPluginRegistry;

#[async_trait]
impl PluginRegistry for OfficialPluginRegistry {
    async fn fetch(
        &self,
        now: DateTime<Utc>,
        cached_revision: Option<u64>,
    ) -> Result<RegistryIndex> {
        RegistryClient::official()?
            .fetch(now, cached_revision)
            .await
    }
}

#[async_trait]
pub trait PluginCommandRuntime: PluginProbe + Clone + Send + Sync {
    async fn configure_plugin(&self, plugin: &InstalledPlugin) -> Result<Value>;
    async fn doctor_plugin(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor>;
}

#[async_trait]
impl PluginCommandRuntime for PluginRuntime {
    async fn configure_plugin(&self, plugin: &InstalledPlugin) -> Result<Value> {
        self.configure(plugin).await
    }

    async fn doctor_plugin(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        self.doctor(plugin).await
    }
}

#[derive(Debug, Clone)]
pub struct PluginCommandContext<R, P> {
    registry: R,
    store: InstalledPluginStore,
    runtime: P,
    registry_state: RegistryRevisionStore,
}

impl<R, P> PluginCommandContext<R, P> {
    pub fn new(registry: R, store: InstalledPluginStore, runtime: P) -> Self {
        let registry_state = RegistryRevisionStore::new(store.paths().clone());
        Self {
            registry,
            store,
            runtime,
            registry_state,
        }
    }
}

pub type ProductionPluginCommandContext =
    PluginCommandContext<OfficialPluginRegistry, PluginRuntime>;

pub fn production_context() -> Result<ProductionPluginCommandContext> {
    let store = InstalledPluginStore::new(PluginPaths::system()?);
    Ok(PluginCommandContext::new(
        OfficialPluginRegistry,
        store,
        PluginRuntime,
    ))
}

pub async fn execute<R, P>(
    context: &PluginCommandContext<R, P>,
    command: &PluginCommands,
) -> Result<Vec<String>>
where
    R: PluginRegistry,
    P: PluginCommandRuntime,
{
    match command {
        PluginCommands::Install { plugin } => install(context, plugin).await,
        PluginCommands::List => list(context),
        PluginCommands::Update { name } => update(context, name).await,
        PluginCommands::Configure { name } => configure(context, name).await,
        PluginCommands::Doctor { name } => doctor(context, name).await,
        PluginCommands::Uninstall { name } => uninstall(context, name),
    }
}

async fn install<R, P>(
    context: &PluginCommandContext<R, P>,
    reference: &PluginReference,
) -> Result<Vec<String>>
where
    R: PluginRegistry,
    P: PluginCommandRuntime,
{
    let index = fetch_registry(context).await?;
    let resolved = resolve_plugin(
        &index,
        &reference.name,
        reference.version.as_ref(),
        current_plugin_target()?,
    )?;
    let installer = PluginInstaller::new(context.store.clone(), context.runtime.clone())?;
    let outcome = installer.install(&resolved).await?;
    Ok(vec![format_install_outcome(&outcome)])
}

async fn update<R, P>(context: &PluginCommandContext<R, P>, name: &str) -> Result<Vec<String>>
where
    R: PluginRegistry,
    P: PluginCommandRuntime,
{
    validate_plugin_name(name)?;
    let _installed = installed_record(context, name)?;
    let index = fetch_registry(context).await?;
    let resolved = resolve_plugin(&index, name, None, current_plugin_target()?)?;
    let installer = PluginInstaller::new(context.store.clone(), context.runtime.clone())?;
    let outcome = installer.update(&resolved).await?;
    Ok(vec![format_install_outcome(&outcome)])
}

fn list<R, P>(context: &PluginCommandContext<R, P>) -> Result<Vec<String>> {
    let plugins = context.store.list()?;
    if plugins.is_empty() {
        return Ok(vec!["No plugins installed".to_string()]);
    }
    Ok(plugins
        .into_iter()
        .map(|plugin| {
            let sources = plugin
                .manifest
                .sources
                .iter()
                .map(|source| source.name.as_str())
                .collect::<Vec<_>>()
                .join(",");
            format!("{} {} [{}]", plugin.name, plugin.version, sources)
        })
        .collect())
}

async fn configure<R, P>(context: &PluginCommandContext<R, P>, name: &str) -> Result<Vec<String>>
where
    P: PluginCommandRuntime,
{
    let plugin = runtime_plugin(context, name)?;
    context.runtime.configure_plugin(&plugin).await?;
    Ok(vec![format!(
        "Configured plugin '{}' {}",
        plugin.name, plugin.version
    )])
}

async fn doctor<R, P>(context: &PluginCommandContext<R, P>, name: &str) -> Result<Vec<String>>
where
    P: PluginCommandRuntime,
{
    let plugin = runtime_plugin(context, name)?;
    let descriptor = context.runtime.doctor_plugin(&plugin).await?;
    let stale = stale_staging_paths(&context.store, &plugin.name)?;
    let mut lines = vec![format!(
        "Plugin '{}' {} is healthy ({} source(s))",
        plugin.name,
        plugin.version,
        descriptor.sources.len()
    )];
    for path in stale {
        lines.push(format!("Stale staging directory: {}", path.display()));
    }
    Ok(lines)
}

fn uninstall<R, P>(context: &PluginCommandContext<R, P>, name: &str) -> Result<Vec<String>>
where
    P: PluginCommandRuntime,
{
    let plugin = installed_record(context, name)?;
    if !context.store.uninstall(&plugin.name)? {
        bail!("Plugin '{}' is not installed", plugin.name);
    }
    Ok(vec![format!(
        "Uninstalled plugin '{}' {}. article-collector cannot recover it; reinstall it from the registry to restore it.",
        plugin.name, plugin.version
    )])
}

async fn fetch_registry<R, P>(context: &PluginCommandContext<R, P>) -> Result<RegistryIndex>
where
    R: PluginRegistry,
{
    let cached_revision = context.registry_state.load()?;
    let index = context.registry.fetch(Utc::now(), cached_revision).await?;
    context.registry_state.record(index.revision)?;
    Ok(index)
}

fn installed_record<R, P>(
    context: &PluginCommandContext<R, P>,
    name: &str,
) -> Result<InstalledPlugin> {
    validate_plugin_name(name)?;
    context
        .store
        .get(name)?
        .with_context(|| format!("Plugin '{name}' is not installed"))
}

fn runtime_plugin<R, P>(
    context: &PluginCommandContext<R, P>,
    name: &str,
) -> Result<InstalledPlugin> {
    context
        .store
        .resolve_runtime_plugin(installed_record(context, name)?)
}

fn format_install_outcome(outcome: &InstallOutcome) -> String {
    let plugin = outcome.installed();
    match outcome {
        InstallOutcome::Installed(_) => {
            format!("Installed plugin '{}' {}", plugin.name, outcome.version())
        }
        InstallOutcome::Updated(_) => {
            format!("Updated plugin '{}' to {}", plugin.name, outcome.version())
        }
        InstallOutcome::AlreadyCurrent(_) => {
            format!(
                "Plugin '{}' is already current at {}",
                plugin.name,
                outcome.version()
            )
        }
    }
}

fn stale_staging_paths(
    store: &InstalledPluginStore,
    name: &str,
) -> Result<Vec<std::path::PathBuf>> {
    let plugin_root = store.paths().plugin_root(name)?;
    let entries = match fs::read_dir(&plugin_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Failed to inspect plugin staging directories"),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        store.paths().ensure_safe_plugin_path(&path)?;
        if entry.file_type()?.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".staging-"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Debug, Clone)]
struct RegistryRevisionStore {
    paths: PluginPaths,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryRevisionState {
    schema_version: u32,
    revision: u64,
}

impl RegistryRevisionStore {
    fn new(paths: PluginPaths) -> Self {
        Self { paths }
    }

    fn load(&self) -> Result<Option<u64>> {
        let path = self.paths.registry_root().join("state.json");
        self.paths.ensure_safe_plugin_path(&path)?;
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read registry state {}", path.display()))
            }
        };
        let state: RegistryRevisionState = serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid registry state {}", path.display()))?;
        if state.schema_version != 1 {
            bail!(
                "Unsupported plugin registry state schema {}",
                state.schema_version
            );
        }
        Ok(Some(state.revision))
    }

    fn record(&self, revision: u64) -> Result<()> {
        if self
            .load()?
            .is_some_and(|cached_revision| revision < cached_revision)
        {
            bail!("Plugin registry revision cannot decrease");
        }
        let root = self.paths.registry_root();
        self.paths.ensure_safe_plugin_path(&root)?;
        fs::create_dir_all(&root).with_context(|| {
            format!(
                "Failed to create registry state directory {}",
                root.display()
            )
        })?;
        self.paths.ensure_safe_plugin_path(&root)?;
        let mut temp = NamedTempFile::new_in(&root)
            .with_context(|| format!("Failed to stage registry state in {}", root.display()))?;
        serde_json::to_writer_pretty(
            temp.as_file_mut(),
            &RegistryRevisionState {
                schema_version: 1,
                revision,
            },
        )?;
        temp.as_file_mut().write_all(b"\n")?;
        temp.as_file().sync_all()?;
        let path = root.join("state.json");
        temp.persist(&path)
            .map_err(|error| error.error)
            .with_context(|| format!("Failed to activate registry state {}", path.display()))?;
        sync_directory(&root)?;
        Ok(())
    }
}

fn sync_directory(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .with_context(|| format!("Failed to open directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("Failed to sync directory {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::types::{
        ManifestSource, PluginManifest, PluginSourceDescriptor, RegistryArtifact, RegistryPlugin,
        RegistryPluginVersion,
    };
    use semver::VersionReq;
    use serde_json::json;
    use std::collections::BTreeMap;
    use url::Url;

    #[derive(Clone)]
    struct FixtureRegistry {
        index: RegistryIndex,
    }

    #[async_trait]
    impl PluginRegistry for FixtureRegistry {
        async fn fetch(
            &self,
            _now: DateTime<Utc>,
            cached_revision: Option<u64>,
        ) -> Result<RegistryIndex> {
            if cached_revision.is_some_and(|revision| self.index.revision < revision) {
                bail!("fixture registry rollback");
            }
            Ok(self.index.clone())
        }
    }

    #[derive(Clone, Default)]
    struct FixtureRuntime {
        doctor_failure: bool,
    }

    #[async_trait]
    impl PluginProbe for FixtureRuntime {
        async fn verify(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
            descriptor(plugin)
        }
    }

    #[async_trait]
    impl PluginCommandRuntime for FixtureRuntime {
        async fn configure_plugin(&self, plugin: &InstalledPlugin) -> Result<Value> {
            if !plugin.manifest.supports_configuration {
                bail!("Plugin '{}' does not support configuration", plugin.name);
            }
            if !plugin.executable.is_absolute() {
                bail!("fixture received unresolved executable path");
            }
            Ok(json!({"status": "configured"}))
        }

        async fn doctor_plugin(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
            if !plugin.executable.is_absolute() {
                bail!("fixture received unresolved executable path");
            }
            if self.doctor_failure {
                bail!("fixture descriptor failure");
            }
            descriptor(plugin)
        }
    }

    fn manifest(name: &str, version: &str, supports_configuration: bool) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: Version::parse(version).unwrap(),
            protocol: VersionReq::parse("^1.0").unwrap(),
            capabilities: vec!["recommend-source".to_string()],
            sources: vec![ManifestSource {
                name: format!("{name}-source"),
                aliases: vec![],
                options_schema: json!({"type":"object","additionalProperties":false}),
            }],
            supports_configuration,
            environment: vec![],
        }
    }

    fn installed(name: &str, version: &str, supports_configuration: bool) -> InstalledPlugin {
        InstalledPlugin {
            schema_version: 1,
            registry_revision: 7,
            name: name.to_string(),
            version: Version::parse(version).unwrap(),
            executable: format!("{version}/plugin").into(),
            manifest: manifest(name, version, supports_configuration),
        }
    }

    fn descriptor(plugin: &InstalledPlugin) -> Result<PluginDescriptor> {
        Ok(PluginDescriptor {
            protocol_version: Version::parse("1.0.0").unwrap(),
            name: plugin.name.clone(),
            version: plugin.version.clone(),
            sources: plugin
                .manifest
                .sources
                .iter()
                .map(|source| PluginSourceDescriptor {
                    name: source.name.clone(),
                    aliases: source.aliases.clone(),
                    options_schema: source.options_schema.clone(),
                })
                .collect(),
            supports_configuration: plugin.manifest.supports_configuration,
        })
    }

    fn registry(plugins: &[(&str, &str)]) -> RegistryIndex {
        let target = current_plugin_target().unwrap().to_string();
        RegistryIndex {
            schema_version: 1,
            revision: 8,
            generated_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::days(1),
            plugins: plugins
                .iter()
                .map(|(name, version)| RegistryPlugin {
                    name: (*name).to_string(),
                    versions: vec![RegistryPluginVersion {
                        version: Version::parse(version).unwrap(),
                        protocol: VersionReq::parse("^1.0").unwrap(),
                        manifest: manifest(name, version, false),
                        artifacts: BTreeMap::from([(
                            target.clone(),
                            RegistryArtifact {
                                url: Url::parse("https://example.com/plugin").unwrap(),
                                byte_length: 1,
                                sha256: "00".repeat(32),
                            },
                        )]),
                    }],
                })
                .collect(),
        }
    }

    fn context(
        root: &std::path::Path,
        index: RegistryIndex,
        runtime: FixtureRuntime,
    ) -> PluginCommandContext<FixtureRegistry, FixtureRuntime> {
        let store = InstalledPluginStore::new(PluginPaths::from_data_root(root.to_path_buf()));
        PluginCommandContext::new(FixtureRegistry { index }, store, runtime)
    }

    fn activate(
        context: &PluginCommandContext<FixtureRegistry, FixtureRuntime>,
        plugin: InstalledPlugin,
    ) {
        let executable = context
            .store
            .paths()
            .plugin_root(&plugin.name)
            .unwrap()
            .join(&plugin.executable);
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"fixture").unwrap();
        context.store.activate(&plugin).unwrap();
    }

    #[test]
    fn install_reference_accepts_name_and_exact_version() {
        assert_eq!(
            PluginReference::from_str("example").unwrap(),
            PluginReference {
                name: "example".to_string(),
                version: None
            }
        );
        assert_eq!(
            PluginReference::from_str("example@1.2.3").unwrap().version,
            Some(Version::parse("1.2.3").unwrap())
        );
        assert!(PluginReference::from_str("../example").is_err());
        assert!(PluginReference::from_str("example@latest").is_err());
    }

    #[tokio::test]
    async fn update_reports_already_current_and_caches_registry_revision() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(
            temp.path(),
            registry(&[("example", "1.0.0")]),
            FixtureRuntime::default(),
        );
        activate(&context, installed("example", "1.0.0", false));

        let lines = execute(
            &context,
            &PluginCommands::Update {
                name: "example".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(lines, ["Plugin 'example' is already current at 1.0.0"]);
        assert_eq!(context.registry_state.load().unwrap(), Some(8));
    }

    #[tokio::test]
    async fn install_rejects_unknown_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), registry(&[]), FixtureRuntime::default());

        let error = execute(
            &context,
            &PluginCommands::Install {
                plugin: PluginReference::from_str("unknown").unwrap(),
            },
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("Unknown plugin 'unknown'"));
    }

    #[tokio::test]
    async fn configure_reports_unsupported_plugin_without_starting_it() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), registry(&[]), FixtureRuntime::default());
        activate(&context, installed("example", "1.0.0", false));

        let error = execute(
            &context,
            &PluginCommands::Configure {
                name: "example".to_string(),
            },
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("does not support configuration"));
    }

    #[tokio::test]
    async fn configure_resolves_executable_under_plugin_data_root() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), registry(&[]), FixtureRuntime::default());
        activate(&context, installed("example", "1.0.0", true));

        let lines = execute(
            &context,
            &PluginCommands::Configure {
                name: "example".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(lines, ["Configured plugin 'example' 1.0.0"]);
    }

    #[tokio::test]
    async fn doctor_propagates_descriptor_failure() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(
            temp.path(),
            registry(&[]),
            FixtureRuntime {
                doctor_failure: true,
            },
        );
        activate(&context, installed("example", "1.0.0", false));

        let error = execute(
            &context,
            &PluginCommands::Doctor {
                name: "example".to_string(),
            },
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("descriptor failure"));
    }

    #[tokio::test]
    async fn doctor_reports_stale_staging_directories_in_order() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), registry(&[]), FixtureRuntime::default());
        activate(&context, installed("example", "1.0.0", false));
        let root = context.store.paths().plugin_root("example").unwrap();
        fs::create_dir_all(root.join(".staging-zeta")).unwrap();
        fs::create_dir_all(root.join(".staging-alpha")).unwrap();

        let lines = execute(
            &context,
            &PluginCommands::Doctor {
                name: "example".to_string(),
            },
        )
        .await
        .unwrap();

        assert!(lines[0].contains("is healthy"));
        assert!(lines[1].ends_with(".staging-alpha"));
        assert!(lines[2].ends_with(".staging-zeta"));
    }

    #[tokio::test]
    async fn list_is_deterministic_and_uninstall_is_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), registry(&[]), FixtureRuntime::default());
        activate(&context, installed("zeta", "1.0.0", false));
        activate(&context, installed("alpha", "2.0.0", false));

        let lines = execute(&context, &PluginCommands::List).await.unwrap();
        assert_eq!(lines[0], "alpha 2.0.0 [alpha-source]");
        assert_eq!(lines[1], "zeta 1.0.0 [zeta-source]");

        let lines = execute(
            &context,
            &PluginCommands::Uninstall {
                name: "alpha".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(lines[0].contains("Uninstalled plugin 'alpha' 2.0.0"));
        assert!(lines[0].contains("cannot recover"));
        assert!(context.store.get("alpha").unwrap().is_none());
        assert!(context.store.get("zeta").unwrap().is_some());
    }
}
