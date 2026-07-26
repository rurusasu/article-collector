use crate::plugins::types::{
    PluginManifest, RegistryArtifact, RegistryIndex, RegistrySignatureSet, TrustedKey,
};
use crate::plugins::{PLUGIN_PROTOCOL_VERSION, REGISTRY_SCHEMA_VERSION};
use anyhow::{bail, Context, Result};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures::StreamExt;
use semver::Version;
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

pub const MAX_REGISTRY_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SIGNATURE_BYTES: usize = 64 * 1024;
pub const OFFICIAL_REGISTRY_INDEX_URL: &str =
    "https://github.com/rurusasu/article-collector/releases/download/plugin-registry-v1/plugins-v1.json";
pub const OFFICIAL_REGISTRY_SIGNATURE_URL: &str =
    "https://github.com/rurusasu/article-collector/releases/download/plugin-registry-v1/plugins-v1.sig";

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPlugin {
    pub registry_revision: u64,
    pub name: String,
    pub version: Version,
    pub manifest: PluginManifest,
    pub artifact: RegistryArtifact,
}

#[derive(Debug, Clone)]
pub struct RegistryClient {
    http: reqwest::Client,
    index_url: Url,
    signature_url: Url,
    trusted_keys: Vec<TrustedKey>,
}

impl RegistryClient {
    pub fn new(index_url: Url, signature_url: Url, trusted_keys: Vec<TrustedKey>) -> Result<Self> {
        if index_url.scheme() != "https" || signature_url.scheme() != "https" {
            bail!("Plugin registry URLs must use HTTPS");
        }
        let http = download_http_client(false)?;
        Ok(Self {
            http,
            index_url,
            signature_url,
            trusted_keys,
        })
    }

    pub fn official() -> Result<Self> {
        let trusted_keys: Vec<TrustedKey> =
            serde_json::from_str(include_str!("../../registry/trusted-keys.json"))
                .context("Invalid embedded plugin registry trust set")?;
        if trusted_keys.is_empty() {
            bail!("Official plugin registry trust keys have not been provisioned");
        }
        Self::new(
            Url::parse(OFFICIAL_REGISTRY_INDEX_URL)?,
            Url::parse(OFFICIAL_REGISTRY_SIGNATURE_URL)?,
            trusted_keys,
        )
    }

    pub async fn fetch(
        &self,
        now: DateTime<Utc>,
        cached_revision: Option<u64>,
    ) -> Result<RegistryIndex> {
        let (index_bytes, signature_bytes) = tokio::try_join!(
            bounded_download(&self.http, self.index_url.clone(), MAX_REGISTRY_BYTES),
            bounded_download(&self.http, self.signature_url.clone(), MAX_SIGNATURE_BYTES)
        )?;
        verify_registry(
            &index_bytes,
            &signature_bytes,
            &self.trusted_keys,
            now,
            cached_revision,
        )
    }
}

pub fn verify_registry(
    index_bytes: &[u8],
    signature_bytes: &[u8],
    trusted_keys: &[TrustedKey],
    now: DateTime<Utc>,
    cached_revision: Option<u64>,
) -> Result<RegistryIndex> {
    let signatures: RegistrySignatureSet = serde_json::from_slice(signature_bytes)
        .context("Invalid plugin registry signature envelope")?;
    if signatures.schema_version != REGISTRY_SCHEMA_VERSION {
        bail!(
            "Unsupported plugin registry signature schema {}",
            signatures.schema_version
        );
    }
    verify_any_signature(index_bytes, &signatures, trusted_keys)?;
    let index: RegistryIndex =
        serde_json::from_slice(index_bytes).context("Invalid signed plugin registry JSON")?;
    validate_registry(&index, now, cached_revision)?;
    Ok(index)
}

pub fn resolve_plugin(
    index: &RegistryIndex,
    name: &str,
    pinned_version: Option<&Version>,
    target: &str,
) -> Result<ResolvedPlugin> {
    let plugin = index
        .plugins
        .iter()
        .find(|plugin| plugin.name == name)
        .with_context(|| format!("Unknown plugin '{name}'"))?;
    let core_protocol = Version::parse(PLUGIN_PROTOCOL_VERSION)
        .context("Core plugin protocol version is invalid")?;

    if let Some(pinned_version) = pinned_version {
        let version = plugin
            .versions
            .iter()
            .find(|candidate| &candidate.version == pinned_version)
            .with_context(|| format!("Plugin '{name}' version {pinned_version} was not found"))?;
        if !version.protocol.matches(&core_protocol) {
            bail!(
                "Plugin '{name}' version {pinned_version} has incompatible protocol {}",
                version.protocol
            );
        }
        let artifact = version.artifacts.get(target).with_context(|| {
            format!("Plugin '{name}' version {pinned_version} has no artifact for target {target}")
        })?;
        return Ok(resolved(index, plugin.name.as_str(), version, artifact));
    }

    let protocol_versions = plugin
        .versions
        .iter()
        .filter(|version| {
            version.version.pre.is_empty() && version.protocol.matches(&core_protocol)
        })
        .collect::<Vec<_>>();
    if protocol_versions.is_empty() {
        bail!("Plugin '{name}' has no stable version compatible with protocol {core_protocol}");
    }
    let version = protocol_versions
        .into_iter()
        .filter(|version| version.artifacts.contains_key(target))
        .max_by(|left, right| left.version.cmp(&right.version))
        .with_context(|| {
            format!("Plugin '{name}' has no compatible artifact for target {target}")
        })?;
    let artifact = version
        .artifacts
        .get(target)
        .expect("filtered for target artifact");
    Ok(resolved(index, plugin.name.as_str(), version, artifact))
}

fn resolved(
    index: &RegistryIndex,
    name: &str,
    version: &crate::plugins::types::RegistryPluginVersion,
    artifact: &RegistryArtifact,
) -> ResolvedPlugin {
    ResolvedPlugin {
        registry_revision: index.revision,
        name: name.to_string(),
        version: version.version.clone(),
        manifest: version.manifest.clone(),
        artifact: artifact.clone(),
    }
}

fn verify_any_signature(
    index_bytes: &[u8],
    signatures: &RegistrySignatureSet,
    trusted_keys: &[TrustedKey],
) -> Result<()> {
    for envelope in &signatures.signatures {
        let Some(key) = trusted_keys
            .iter()
            .find(|key| key.key_id == envelope.key_id && !key.revoked)
        else {
            continue;
        };
        let public_key = BASE64_STANDARD
            .decode(&key.public_key)
            .with_context(|| format!("Invalid public key encoding for '{}'", key.key_id))?;
        let public_key: [u8; 32] = public_key.try_into().map_err(|_| {
            anyhow::anyhow!("Invalid Ed25519 public key length for '{}'", key.key_id)
        })?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .with_context(|| format!("Invalid Ed25519 public key '{}'", key.key_id))?;
        let signature = BASE64_STANDARD
            .decode(&envelope.signature)
            .with_context(|| format!("Invalid signature encoding for '{}'", envelope.key_id))?;
        let signature = Signature::from_slice(&signature)
            .with_context(|| format!("Invalid signature length for '{}'", envelope.key_id))?;
        if verifying_key.verify(index_bytes, &signature).is_ok() {
            return Ok(());
        }
    }
    bail!("Plugin registry has no valid trusted signature")
}

fn validate_registry(
    index: &RegistryIndex,
    now: DateTime<Utc>,
    cached_revision: Option<u64>,
) -> Result<()> {
    if index.schema_version != REGISTRY_SCHEMA_VERSION {
        bail!(
            "Unsupported plugin registry schema {}",
            index.schema_version
        );
    }
    if index.generated_at > now {
        bail!("Plugin registry was generated in the future");
    }
    if index.expires_at <= now {
        bail!("Plugin registry has expired");
    }
    if cached_revision.is_some_and(|revision| index.revision < revision) {
        bail!(
            "Plugin registry rollback detected: revision {} is older than cached revision {}",
            index.revision,
            cached_revision.unwrap_or_default()
        );
    }

    let mut plugin_names = HashSet::new();
    for plugin in &index.plugins {
        if !plugin_names.insert(plugin.name.as_str()) {
            bail!(
                "Plugin registry contains duplicate plugin '{}'",
                plugin.name
            );
        }
        crate::plugins::paths::validate_plugin_name(&plugin.name)?;
        let mut versions = HashSet::new();
        for version in &plugin.versions {
            if !versions.insert(version.version.clone()) {
                bail!(
                    "Plugin registry contains duplicate version {} for '{}'",
                    version.version,
                    plugin.name
                );
            }
            validate_manifest(&plugin.name, version)?;
            for (target, artifact) in &version.artifacts {
                crate::plugins::paths::plugin_target_from_triple(target)?;
                if artifact.url.scheme() != "https" {
                    bail!("Plugin artifact URL must use HTTPS");
                }
                if artifact.byte_length == 0 {
                    bail!("Plugin artifact byte length must be greater than zero");
                }
                if artifact.sha256.len() != 64
                    || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    bail!("Plugin artifact SHA-256 must be 64 hexadecimal characters");
                }
            }
        }
    }
    Ok(())
}

fn validate_manifest(
    plugin_name: &str,
    version: &crate::plugins::types::RegistryPluginVersion,
) -> Result<()> {
    let manifest = &version.manifest;
    if manifest.name != plugin_name || manifest.version != version.version {
        bail!("Plugin manifest identity does not match registry entry");
    }
    if manifest.protocol != version.protocol {
        bail!("Plugin manifest protocol does not match registry entry");
    }
    if manifest.capabilities.as_slice() != ["recommend-source"] {
        bail!("Plugin manifest must declare exactly the recommend-source capability");
    }
    if manifest.sources.is_empty() {
        bail!("Plugin manifest must declare at least one source");
    }
    let mut names = HashSet::new();
    for source in &manifest.sources {
        crate::plugins::paths::validate_plugin_name(&source.name)?;
        jsonschema::validator_for(&source.options_schema).with_context(|| {
            format!(
                "Plugin source '{}' contains an invalid options schema",
                source.name
            )
        })?;
        if !names.insert(source.name.as_str()) {
            bail!(
                "Plugin manifest contains duplicate source '{}'",
                source.name
            );
        }
        for alias in &source.aliases {
            crate::plugins::paths::validate_plugin_name(alias)?;
            if !names.insert(alias.as_str()) {
                bail!("Plugin manifest contains duplicate source alias '{alias}'");
            }
        }
    }
    Ok(())
}

async fn bounded_download(client: &reqwest::Client, url: Url, maximum: usize) -> Result<Vec<u8>> {
    if url.scheme() != "https" {
        bail!("Plugin registry downloads require HTTPS");
    }
    let response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("Failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("Plugin registry request failed for {url}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        bail!("Plugin registry response exceeds {maximum} bytes");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("Failed while downloading {url}"))?;
        if bytes.len().saturating_add(chunk.len()) > maximum {
            bail!("Plugin registry response exceeds {maximum} bytes");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) fn download_http_client(allow_insecure_localhost: bool) -> Result<reqwest::Client> {
    let redirect = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many plugin download redirects")
        } else if download_url_allowed(attempt.url(), allow_insecure_localhost) {
            attempt.follow()
        } else {
            attempt.error("plugin download redirect must use HTTPS")
        }
    });
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(redirect)
        .build()
        .context("Failed to build plugin download HTTP client")
}

fn download_url_allowed(url: &Url, allow_insecure_localhost: bool) -> bool {
    url.scheme() == "https"
        || (allow_insecure_localhost
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::prelude::BASE64_STANDARD;
    use chrono::{DateTime, Utc};
    use ed25519_dalek::{Signature, Signer, SigningKey};
    use semver::Version;
    use serde_json::{json, Value};

    struct SignedFixture {
        index_bytes: Vec<u8>,
        signature_bytes: Vec<u8>,
        trusted_key: TrustedKey,
    }

    fn utc(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    fn version(version: &str, protocol: &str) -> Value {
        json!({
            "version": version,
            "protocol": protocol,
            "manifest": {
                "name": "example",
                "version": version,
                "protocol": protocol,
                "capabilities": ["recommend-source"],
                "sources": [{
                    "name": "example",
                    "aliases": ["sample"],
                    "options_schema": {
                        "type": "object",
                        "additionalProperties": false
                    }
                }],
                "supports_configuration": false,
                "environment": []
            },
            "artifacts": {
                "aarch64-apple-darwin": {
                    "url": format!("https://example.com/example-{version}"),
                    "byte_length": 4,
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                }
            }
        })
    }

    fn registry(versions: Vec<Value>) -> Value {
        json!({
            "schema_version": 1,
            "revision": 7,
            "generated_at": "2026-07-26T00:00:00Z",
            "expires_at": "2026-08-26T00:00:00Z",
            "plugins": [{"name": "example", "versions": versions}]
        })
    }

    fn signed_registry_fixture(index: Value) -> SignedFixture {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let index_bytes = serde_json::to_vec(&index).unwrap();
        let signature: Signature = signing_key.sign(&index_bytes);
        let signature_bytes = serde_json::to_vec(&json!({
            "schema_version": 1,
            "signatures": [{
                "key_id": "fixture-2026",
                "signature": BASE64_STANDARD.encode(signature.to_bytes())
            }]
        }))
        .unwrap();
        SignedFixture {
            index_bytes,
            signature_bytes,
            trusted_key: TrustedKey {
                key_id: "fixture-2026".to_string(),
                public_key: BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes()),
                revoked: false,
            },
        }
    }

    /// 検証: 正しい署名のregistryから最新のstable互換versionを選択する
    /// 理由: registry順序ではなくsemver/protocolに従い決定的にinstallするため
    /// リスク: 非互換またはprereleaseを意図せず選択する
    #[test]
    fn verifies_signed_registry_and_selects_latest_compatible_version() {
        let fixture = signed_registry_fixture(registry(vec![
            version("1.0.0", ">=1.0.0, <2.0.0"),
            version("1.1.0-beta.1", ">=1.0.0, <2.0.0"),
            version("2.0.0", ">=2.0.0, <3.0.0"),
        ]));
        let index = verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[fixture.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            Some(6),
        )
        .unwrap();

        let resolved = resolve_plugin(&index, "example", None, "aarch64-apple-darwin").unwrap();

        assert_eq!(resolved.version, Version::parse("1.0.0").unwrap());
        assert_eq!(resolved.artifact.byte_length, 4);
    }

    /// 検証: JSON末尾の空白を含む1byteの変更でも署名検証を失敗させる
    /// 理由: 再serializeした値ではなく配布された正確なbytesを認証するため
    /// リスク: 署名されていない表現差分を受理する
    #[test]
    fn rejects_any_change_to_signed_registry_bytes() {
        let mut fixture =
            signed_registry_fixture(registry(vec![version("1.0.0", ">=1.0.0, <2.0.0")]));
        fixture.index_bytes.push(b' ');

        let error = verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[fixture.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            None,
        )
        .unwrap_err();

        assert!(error.to_string().contains("signature"));
    }

    #[test]
    fn rejects_unknown_signing_keys() {
        let fixture = signed_registry_fixture(registry(vec![version("1.0.0", ">=1.0.0, <2.0.0")]));
        let unknown = TrustedKey {
            key_id: "different-key".to_string(),
            public_key: fixture.trusted_key.public_key.clone(),
            revoked: false,
        };
        assert!(verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[unknown],
            utc("2026-07-26T12:00:00Z"),
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("trusted signature"));
    }

    #[test]
    fn rejects_expired_and_rolled_back_registry_metadata() {
        let fixture = signed_registry_fixture(registry(vec![version("1.0.0", ">=1.0.0, <2.0.0")]));
        let expired = verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            std::slice::from_ref(&fixture.trusted_key),
            utc("2026-09-01T00:00:00Z"),
            None,
        )
        .unwrap_err();
        assert!(expired.to_string().contains("expired"));

        let rollback = verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[fixture.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            Some(8),
        )
        .unwrap_err();
        assert!(rollback.to_string().contains("rollback"));
    }

    #[test]
    fn rejects_wrong_schema_and_duplicate_semantic_entries() {
        let mut wrong_schema = registry(vec![version("1.0.0", ">=1.0.0, <2.0.0")]);
        wrong_schema["schema_version"] = json!(2);
        let fixture = signed_registry_fixture(wrong_schema);
        assert!(verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[fixture.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("schema"));

        let duplicate = signed_registry_fixture(json!({
            "schema_version": 1,
            "revision": 7,
            "generated_at": "2026-07-26T00:00:00Z",
            "expires_at": "2026-08-26T00:00:00Z",
            "plugins": [
                {"name": "example", "versions": [version("1.0.0", ">=1.0.0, <2.0.0")]},
                {"name": "example", "versions": [version("1.1.0", ">=1.0.0, <2.0.0")]}
            ]
        }));
        assert!(verify_registry(
            &duplicate.index_bytes,
            &duplicate.signature_bytes,
            &[duplicate.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("duplicate plugin"));
    }

    /// 検証:署名済みmanifest内の不正なoptions JSON Schemaをinstall前に拒否する
    /// 理由:publisher以外の署名経路でもruntime contractを早期検証するため
    /// リスク:install/probe後のrecommend planningで初めて壊れる
    #[test]
    fn rejects_invalid_options_json_schema() {
        let mut invalid = version("1.0.0", ">=1.0.0, <2.0.0");
        invalid["manifest"]["sources"][0]["options_schema"] = json!({"type": 42});
        let fixture = signed_registry_fixture(registry(vec![invalid]));

        let error = verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[fixture.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            None,
        )
        .unwrap_err();

        assert!(error.to_string().contains("options schema"));
    }

    /// 検証:redirect先もHTTPS（test localhostを除く）に制限する
    /// 理由:初期URLだけのscheme検査ではHTTP downgradeを防げないため
    /// リスク:registry/artifactの取得先privacyとavailabilityが低下する
    #[test]
    fn rejects_insecure_download_redirect_targets() {
        assert!(download_url_allowed(
            &Url::parse("https://example.com/plugin").unwrap(),
            false
        ));
        assert!(!download_url_allowed(
            &Url::parse("http://example.com/plugin").unwrap(),
            false
        ));
        assert!(download_url_allowed(
            &Url::parse("http://127.0.0.1/plugin").unwrap(),
            true
        ));
    }

    #[test]
    fn resolver_reports_target_protocol_and_pin_errors() {
        let fixture = signed_registry_fixture(registry(vec![
            version("1.0.0", ">=1.0.0, <2.0.0"),
            version("2.0.0", ">=2.0.0, <3.0.0"),
        ]));
        let index = verify_registry(
            &fixture.index_bytes,
            &fixture.signature_bytes,
            &[fixture.trusted_key],
            utc("2026-07-26T12:00:00Z"),
            None,
        )
        .unwrap();

        assert!(
            resolve_plugin(&index, "example", None, "x86_64-pc-windows-msvc")
                .unwrap_err()
                .to_string()
                .contains("target")
        );
        assert!(resolve_plugin(
            &index,
            "example",
            Some(&Version::parse("2.0.0").unwrap()),
            "aarch64-apple-darwin",
        )
        .unwrap_err()
        .to_string()
        .contains("protocol"));
        assert!(resolve_plugin(
            &index,
            "example",
            Some(&Version::parse("9.0.0").unwrap()),
            "aarch64-apple-darwin",
        )
        .unwrap_err()
        .to_string()
        .contains("version"));
    }
}
