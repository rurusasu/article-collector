use chrono::{DateTime, Utc};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub revision: u64,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub plugins: Vec<RegistryPlugin>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RegistryPlugin {
    pub name: String,
    pub versions: Vec<RegistryPluginVersion>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RegistryPluginVersion {
    pub version: Version,
    pub protocol: VersionReq,
    pub manifest: PluginManifest,
    pub artifacts: BTreeMap<String, RegistryArtifact>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RegistryArtifact {
    pub url: Url,
    pub byte_length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistrySignatureSet {
    pub schema_version: u32,
    pub signatures: Vec<RegistrySignature>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistrySignature {
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedKey {
    pub key_id: String,
    pub public_key: String,
    #[serde(default)]
    pub revoked: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub name: String,
    pub version: Version,
    pub protocol: VersionReq,
    pub capabilities: Vec<String>,
    pub sources: Vec<ManifestSource>,
    pub supports_configuration: bool,
    #[serde(default)]
    pub environment: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestSource {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub options_schema: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InstalledPlugin {
    pub schema_version: u32,
    pub registry_revision: u64,
    pub name: String,
    pub version: Version,
    pub executable: PathBuf,
    pub manifest: PluginManifest,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginDescriptor {
    pub protocol_version: Version,
    pub name: String,
    pub version: Version,
    pub sources: Vec<PluginSourceDescriptor>,
    pub supports_configuration: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginSourceDescriptor {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub options_schema: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollectRequest {
    pub source: String,
    pub limit: usize,
    #[serde(default)]
    pub options: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollectResponse {
    pub items: Vec<PluginCandidate>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginCandidate {
    pub title: String,
    pub url: Url,
    pub content: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 検証: registry schema v1 は未知フィールドを拒否する
    /// 理由: 署名対象の意味を曖昧にせず、未対応 schema を黙って受理しないため
    /// リスク: 新旧クライアントで署名済み registry の解釈が分岐する
    #[test]
    fn registry_contract_rejects_unknown_fields() {
        let raw = r#"{
          "schema_version":1,
          "revision":1,
          "generated_at":"2026-07-26T00:00:00Z",
          "expires_at":"2026-08-26T00:00:00Z",
          "plugins":[],
          "unexpected":true
        }"#;

        assert!(serde_json::from_str::<RegistryIndex>(raw).is_err());
    }
}
