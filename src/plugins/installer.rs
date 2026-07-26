use crate::plugins::registry::{download_http_client, ResolvedPlugin};
use crate::plugins::state::InstalledPluginStore;
use crate::plugins::types::{InstalledPlugin, PluginDescriptor, PluginManifest, RegistryArtifact};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use semver::Version;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait PluginProbe: Send + Sync {
    async fn verify(&self, plugin: &InstalledPlugin) -> Result<PluginDescriptor>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallOutcome {
    Installed(InstalledPlugin),
    Updated(InstalledPlugin),
    AlreadyCurrent(InstalledPlugin),
}

impl InstallOutcome {
    pub fn version(&self) -> &Version {
        match self {
            Self::Installed(plugin) | Self::Updated(plugin) | Self::AlreadyCurrent(plugin) => {
                &plugin.version
            }
        }
    }

    pub fn installed(&self) -> &InstalledPlugin {
        match self {
            Self::Installed(plugin) | Self::Updated(plugin) | Self::AlreadyCurrent(plugin) => {
                plugin
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginInstaller<P> {
    store: InstalledPluginStore,
    probe: P,
    http: reqwest::Client,
    allow_insecure_localhost: bool,
}

impl<P> PluginInstaller<P>
where
    P: PluginProbe,
{
    pub fn new(store: InstalledPluginStore, probe: P) -> Result<Self> {
        Ok(Self {
            store,
            probe,
            http: download_http_client(false)?,
            allow_insecure_localhost: false,
        })
    }

    #[cfg(test)]
    fn new_for_tests(store: InstalledPluginStore, probe: P) -> Self {
        Self {
            store,
            probe,
            http: download_http_client(true).expect("test HTTP client"),
            allow_insecure_localhost: true,
        }
    }

    pub async fn install(&self, resolved: &ResolvedPlugin) -> Result<InstallOutcome> {
        if let Some(active) = self.store.get(&resolved.name)? {
            if active.version == resolved.version {
                return Ok(InstallOutcome::AlreadyCurrent(active));
            }
            bail!(
                "Plugin '{}' is already installed at version {}; use plugin update",
                resolved.name,
                active.version
            );
        }
        let installed = self.install_transaction(resolved).await?;
        Ok(InstallOutcome::Installed(installed))
    }

    pub async fn update(&self, resolved: &ResolvedPlugin) -> Result<InstallOutcome> {
        let active = self
            .store
            .get(&resolved.name)?
            .with_context(|| format!("Plugin '{}' is not installed", resolved.name))?;
        if active.version == resolved.version {
            return Ok(InstallOutcome::AlreadyCurrent(active));
        }
        let installed = self.install_transaction(resolved).await?;
        Ok(InstallOutcome::Updated(installed))
    }

    async fn install_transaction(&self, resolved: &ResolvedPlugin) -> Result<InstalledPlugin> {
        let plugin_root = self.store.paths().plugin_root(&resolved.name)?;
        fs::create_dir_all(&plugin_root)
            .with_context(|| format!("Failed to create {}", plugin_root.display()))?;
        self.store.paths().ensure_safe_plugin_path(&plugin_root)?;
        let staging = tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&plugin_root)
            .with_context(|| {
                format!("Failed to create staging directory for '{}'", resolved.name)
            })?;
        let executable_name = if cfg!(windows) {
            "plugin.exe"
        } else {
            "plugin"
        };
        let staged_executable = staging.path().join(executable_name);

        self.download_and_verify(&resolved.artifact, &staged_executable)
            .await?;
        make_executable(&staged_executable)?;
        write_manifest(staging.path(), &resolved.manifest)?;

        let probe_plugin = installed_record(
            resolved,
            staged_executable
                .canonicalize()
                .with_context(|| format!("Failed to resolve {}", staged_executable.display()))?,
        );
        let descriptor = self.probe.verify(&probe_plugin).await?;
        validate_descriptor(&resolved.manifest, &descriptor)?;

        let final_root = plugin_root.join(resolved.version.to_string());
        self.store.paths().ensure_safe_plugin_path(&final_root)?;
        if final_root.exists() {
            bail!(
                "Plugin '{}' version {} directory already exists",
                resolved.name,
                resolved.version
            );
        }
        let staging_path = staging.keep();
        fs::rename(&staging_path, &final_root).with_context(|| {
            format!(
                "Failed to commit plugin version from {} to {}",
                staging_path.display(),
                final_root.display()
            )
        })?;

        let installed = installed_record(
            resolved,
            PathBuf::from(resolved.version.to_string()).join(executable_name),
        );
        if let Err(error) = self.store.activate(&installed) {
            let _ = fs::remove_dir_all(&final_root);
            return Err(error).context("Failed to activate installed plugin");
        }
        Ok(installed)
    }

    async fn download_and_verify(&self, artifact: &RegistryArtifact, path: &Path) -> Result<()> {
        if artifact.url.scheme() != "https"
            && !(self.allow_insecure_localhost
                && artifact.url.scheme() == "http"
                && matches!(artifact.url.host_str(), Some("127.0.0.1" | "localhost")))
        {
            bail!("Plugin artifact URL must use HTTPS");
        }
        let response = self
            .http
            .get(artifact.url.clone())
            .send()
            .await
            .with_context(|| format!("Failed to download plugin artifact {}", artifact.url))?
            .error_for_status()
            .with_context(|| format!("Plugin artifact request failed for {}", artifact.url))?;
        if response
            .content_length()
            .is_some_and(|length| length != artifact.byte_length)
        {
            bail!(
                "Plugin artifact length mismatch: expected {}, response declared {}",
                artifact.byte_length,
                response.content_length().unwrap_or_default()
            );
        }

        let mut file = tokio::fs::File::create(path)
            .await
            .with_context(|| format!("Failed to create {}", path.display()))?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut byte_count = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.with_context(|| format!("Interrupted artifact download {}", artifact.url))?;
            byte_count = byte_count
                .checked_add(chunk.len() as u64)
                .context("Plugin artifact byte count overflow")?;
            if byte_count > artifact.byte_length {
                bail!(
                    "Plugin artifact exceeds signed length {}",
                    artifact.byte_length
                );
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .with_context(|| format!("Failed to write {}", path.display()))?;
        }
        file.sync_all()
            .await
            .with_context(|| format!("Failed to sync {}", path.display()))?;
        if byte_count != artifact.byte_length {
            bail!(
                "Plugin artifact length mismatch: expected {}, downloaded {}",
                artifact.byte_length,
                byte_count
            );
        }
        let actual = hex_lower(hasher.finalize().as_slice());
        if !actual.eq_ignore_ascii_case(&artifact.sha256) {
            bail!(
                "Plugin artifact SHA-256 mismatch: expected {}, got {}",
                artifact.sha256,
                actual
            );
        }
        Ok(())
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn installed_record(resolved: &ResolvedPlugin, executable: PathBuf) -> InstalledPlugin {
    InstalledPlugin {
        schema_version: 1,
        registry_revision: resolved.registry_revision,
        name: resolved.name.clone(),
        version: resolved.version.clone(),
        executable,
        manifest: resolved.manifest.clone(),
    }
}

fn write_manifest(staging_root: &Path, manifest: &PluginManifest) -> Result<()> {
    let encoded = toml::to_string_pretty(manifest).context("Failed to encode plugin manifest")?;
    let path = staging_root.join("plugin.toml");
    fs::write(&path, encoded).with_context(|| format!("Failed to write {}", path.display()))?;
    FileSync::sync(&path)
}

struct FileSync;

impl FileSync {
    fn sync(path: &Path) -> Result<()> {
        fs::File::open(path)
            .with_context(|| format!("Failed to open {}", path.display()))?
            .sync_all()
            .with_context(|| format!("Failed to sync {}", path.display()))
    }
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("Failed to mark {} executable", path.display()))?;
    }
    Ok(())
}

pub fn validate_descriptor(manifest: &PluginManifest, descriptor: &PluginDescriptor) -> Result<()> {
    if descriptor.name != manifest.name
        || descriptor.version != manifest.version
        || !manifest.protocol.matches(&descriptor.protocol_version)
        || descriptor.supports_configuration != manifest.supports_configuration
    {
        bail!("Plugin runtime descriptor identity does not match signed manifest");
    }
    if descriptor.sources.len() != manifest.sources.len() {
        bail!("Plugin runtime descriptor sources do not match signed manifest");
    }
    for (expected, actual) in manifest.sources.iter().zip(&descriptor.sources) {
        if expected.name != actual.name
            || expected.aliases != actual.aliases
            || expected.options_schema != actual.options_schema
        {
            bail!("Plugin runtime descriptor sources do not match signed manifest");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::paths::PluginPaths;
    use crate::plugins::registry::ResolvedPlugin;
    use crate::plugins::state::InstalledPluginStore;
    use crate::plugins::types::{
        ManifestSource, PluginDescriptor, PluginManifest, PluginSourceDescriptor, RegistryArtifact,
    };
    use semver::{Version, VersionReq};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct FixtureProbe {
        descriptor: PluginDescriptor,
    }

    #[async_trait::async_trait]
    impl PluginProbe for FixtureProbe {
        async fn verify(
            &self,
            _plugin: &crate::plugins::types::InstalledPlugin,
        ) -> anyhow::Result<PluginDescriptor> {
            Ok(self.descriptor.clone())
        }
    }

    fn manifest(version: &str) -> PluginManifest {
        PluginManifest {
            name: "example".to_string(),
            version: Version::parse(version).unwrap(),
            protocol: VersionReq::parse(">=1.0.0, <2.0.0").unwrap(),
            capabilities: vec!["recommend-source".to_string()],
            sources: vec![ManifestSource {
                name: "example".to_string(),
                aliases: vec!["sample".to_string()],
                options_schema: json!({"type": "object", "additionalProperties": false}),
            }],
            supports_configuration: false,
            environment: Vec::new(),
        }
    }

    fn descriptor(version: &str) -> PluginDescriptor {
        PluginDescriptor {
            protocol_version: Version::parse("1.0.0").unwrap(),
            name: "example".to_string(),
            version: Version::parse(version).unwrap(),
            sources: vec![PluginSourceDescriptor {
                name: "example".to_string(),
                aliases: vec!["sample".to_string()],
                options_schema: json!({"type": "object", "additionalProperties": false}),
            }],
            supports_configuration: false,
        }
    }

    async fn serve_once(body: Arc<Vec<u8>>) -> url::Url {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        });
        url::Url::parse(&format!("http://{address}/plugin")).unwrap()
    }

    async fn resolved(version: &str, bytes: Arc<Vec<u8>>) -> ResolvedPlugin {
        let digest = hex_lower(Sha256::digest(bytes.as_slice()).as_slice());
        ResolvedPlugin {
            registry_revision: 7,
            name: "example".to_string(),
            version: Version::parse(version).unwrap(),
            manifest: manifest(version),
            artifact: RegistryArtifact {
                url: serve_once(bytes.clone()).await,
                byte_length: bytes.len() as u64,
                sha256: digest,
            },
        }
    }

    fn fixture(
        root: &std::path::Path,
        version: &str,
    ) -> (
        InstalledPluginStore,
        PluginInstaller<FixtureProbe>,
        PluginPaths,
    ) {
        let paths = PluginPaths::from_data_root(root.to_path_buf());
        let store = InstalledPluginStore::new(paths.clone());
        let installer = PluginInstaller::new_for_tests(
            store.clone(),
            FixtureProbe {
                descriptor: descriptor(version),
            },
        );
        (store, installer, paths)
    }

    /// 検証: download/hash/probe成功後だけactive versionを切り替える
    /// 理由: 壊れた実行ファイルをinstalled.jsonから参照させないため
    /// リスク: install成功表示後にpluginが起動不能になる
    #[tokio::test]
    async fn install_hashes_probes_and_atomically_activates_version() {
        let temp = tempfile::tempdir().unwrap();
        let (store, installer, paths) = fixture(temp.path(), "1.1.0");
        let resolved = resolved("1.1.0", Arc::new(b"new executable".to_vec())).await;

        let outcome = installer.install(&resolved).await.unwrap();

        assert_eq!(outcome.version(), &Version::parse("1.1.0").unwrap());
        assert_eq!(
            store.get("example").unwrap().unwrap().version,
            Version::parse("1.1.0").unwrap()
        );
        assert!(!has_staging_directory(&paths, "example"));
    }

    /// 検証: hash不一致時は旧active versionを保持しstagingを除去する
    /// 理由: 改ざん・転送破損したartifactを有効化しないため
    /// リスク: update失敗で正常な旧versionまで利用不能になる
    #[tokio::test]
    async fn wrong_hash_preserves_old_active_version() {
        let temp = tempfile::tempdir().unwrap();
        let (store, installer, paths) = fixture(temp.path(), "1.1.0");
        let old = resolved("1.0.0", Arc::new(b"old executable".to_vec())).await;
        PluginInstaller::new_for_tests(
            store.clone(),
            FixtureProbe {
                descriptor: descriptor("1.0.0"),
            },
        )
        .install(&old)
        .await
        .unwrap();

        let mut update = resolved("1.1.0", Arc::new(b"new executable".to_vec())).await;
        update.artifact.sha256 = "0".repeat(64);
        let error = installer.update(&update).await.unwrap_err();

        assert!(error.to_string().contains("SHA-256"));
        assert_eq!(
            store.get("example").unwrap().unwrap().version,
            Version::parse("1.0.0").unwrap()
        );
        assert!(!has_staging_directory(&paths, "example"));
    }

    /// 検証: 同じversionへのupdateはnetwork/probeを行わずno-opになる
    /// 理由: updateを冪等にし不要な外部処理を避けるため
    /// リスク: 定期updateが毎回artifactを再配置する
    #[tokio::test]
    async fn update_of_current_version_is_noop() {
        let temp = tempfile::tempdir().unwrap();
        let (store, installer, _) = fixture(temp.path(), "1.0.0");
        let initial = resolved("1.0.0", Arc::new(b"executable".to_vec())).await;
        installer.install(&initial).await.unwrap();
        let unreachable = ResolvedPlugin {
            artifact: RegistryArtifact {
                url: url::Url::parse("http://127.0.0.1:1/unreachable").unwrap(),
                ..initial.artifact.clone()
            },
            ..initial
        };

        let outcome = installer.update(&unreachable).await.unwrap();

        assert!(matches!(outcome, InstallOutcome::AlreadyCurrent(_)));
        assert!(store.get("example").unwrap().is_some());
    }

    /// 検証: probe descriptorが署名manifestと違う場合は有効化しない
    /// 理由: 配布artifactと署名済みsource所有情報を実行時に照合するため
    /// リスク: 差し替えられたbinaryが別sourceとして実行される
    #[tokio::test]
    async fn probe_mismatch_does_not_activate_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let paths = PluginPaths::from_data_root(temp.path().to_path_buf());
        let store = InstalledPluginStore::new(paths.clone());
        let installer = PluginInstaller::new_for_tests(
            store.clone(),
            FixtureProbe {
                descriptor: descriptor("9.0.0"),
            },
        );
        let candidate = resolved("1.0.0", Arc::new(b"executable".to_vec())).await;

        let error = installer.install(&candidate).await.unwrap_err();

        assert!(error.to_string().contains("descriptor"));
        assert!(store.get("example").unwrap().is_none());
        assert!(!has_staging_directory(&paths, "example"));
    }

    fn has_staging_directory(paths: &PluginPaths, name: &str) -> bool {
        let Ok(root) = paths.plugin_root(name) else {
            return false;
        };
        std::fs::read_dir(root).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with(".staging-"))
        })
    }
}
