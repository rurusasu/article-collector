// Reuse the production wire types; this publisher needs only their registry subset.
#[allow(dead_code)]
#[path = "../src/plugins/types.rs"]
mod types;

use anyhow::{bail, Context, Result};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use types::{
    RegistryIndex, RegistryPluginVersion, RegistrySignature, RegistrySignatureSet, TrustedKey,
};

const REGISTRY_SCHEMA_VERSION: u32 = 1;
const MAX_REGISTRY_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 64 * 1024;
const SIGNING_KEY_ENV: &str = "PLUGIN_REGISTRY_SIGNING_KEY";

#[derive(Debug, Parser)]
#[command(about = "Validate, sign, and verify article-collector plugin registries")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate registry and public trust-set schemas without private material
    Validate {
        index: PathBuf,
        trusted_keys: PathBuf,
    },
    /// Sign the exact registry bytes with PLUGIN_REGISTRY_SIGNING_KEY
    Sign {
        index: PathBuf,
        signature_output: PathBuf,
    },
    /// Verify exact registry bytes against a signature envelope and trust set
    Verify {
        index: PathBuf,
        signature: PathBuf,
        trusted_keys: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Validate {
            index,
            trusted_keys,
        } => {
            let index_bytes = read_bounded(&index, MAX_REGISTRY_BYTES)?;
            let keys = read_trusted_keys(&trusted_keys)?;
            validate_registry_bytes(&index_bytes, Utc::now())?;
            validate_trusted_keys(&keys)?;
            println!("Plugin registry and trust set are valid.");
        }
        Commands::Sign {
            index,
            signature_output,
        } => {
            let index_bytes = read_bounded(&index, MAX_REGISTRY_BYTES)?;
            validate_registry_bytes(&index_bytes, Utc::now())?;
            let seed = signing_seed_from_environment()?;
            let signatures = sign_bytes(&index_bytes, &seed);
            write_json(&signature_output, &signatures)?;
            println!("Signed plugin registry -> {}", signature_output.display());
        }
        Commands::Verify {
            index,
            signature,
            trusted_keys,
        } => {
            let index_bytes = read_bounded(&index, MAX_REGISTRY_BYTES)?;
            let signature_bytes = read_bounded(&signature, MAX_SIGNATURE_BYTES)?;
            let keys = read_trusted_keys(&trusted_keys)?;
            validate_registry_bytes(&index_bytes, Utc::now())?;
            validate_trusted_keys(&keys)?;
            verify_bytes(&index_bytes, &signature_bytes, &keys)?;
            println!("Plugin registry signature is valid.");
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to inspect {}", path.display()))?;
    if metadata.len() > maximum {
        bail!(
            "{} is too large: {} bytes exceeds {maximum}",
            path.display(),
            metadata.len()
        );
    }
    fs::read(path).with_context(|| format!("Failed to read {}", path.display()))
}

fn read_trusted_keys(path: &Path) -> Result<Vec<TrustedKey>> {
    let bytes = read_bounded(path, MAX_SIGNATURE_BYTES)?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("Invalid trusted key JSON {}", path.display()))
}

fn validate_registry_bytes(bytes: &[u8], now: DateTime<Utc>) -> Result<RegistryIndex> {
    let index: RegistryIndex =
        serde_json::from_slice(bytes).context("Invalid plugin registry JSON")?;
    validate_registry(&index, now)?;
    Ok(index)
}

fn validate_registry(index: &RegistryIndex, now: DateTime<Utc>) -> Result<()> {
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

    let mut plugin_names = HashSet::new();
    let mut source_owners = HashMap::new();
    for plugin in &index.plugins {
        validate_name(&plugin.name)?;
        if !plugin_names.insert(plugin.name.as_str()) {
            bail!(
                "Plugin registry contains duplicate plugin '{}'",
                plugin.name
            );
        }
        let mut versions = HashSet::new();
        for version in &plugin.versions {
            if !versions.insert(version.version.clone()) {
                bail!(
                    "Plugin registry contains duplicate version {} for '{}'",
                    version.version,
                    plugin.name
                );
            }
            validate_manifest(&plugin.name, version, &mut source_owners)?;
            for (target, artifact) in &version.artifacts {
                validate_target(target)?;
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
    version: &RegistryPluginVersion,
    registry_sources: &mut HashMap<String, String>,
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

    let mut manifest_names = HashSet::new();
    for source in &manifest.sources {
        validate_name(&source.name)?;
        if !manifest_names.insert(source.name.as_str()) {
            bail!(
                "Plugin manifest contains duplicate source '{}'",
                source.name
            );
        }
        claim_source(registry_sources, &source.name, plugin_name, "name")?;
        validate_options_schema(&source.options_schema)?;
        for alias in &source.aliases {
            validate_name(alias)?;
            if !manifest_names.insert(alias.as_str()) {
                bail!("Plugin manifest contains duplicate source alias '{alias}'");
            }
            claim_source(registry_sources, alias, plugin_name, "alias")?;
        }
    }
    Ok(())
}

fn claim_source(
    sources: &mut HashMap<String, String>,
    name: &str,
    plugin_name: &str,
    kind: &str,
) -> Result<()> {
    if sources.get(name).is_some_and(|owner| owner != plugin_name) {
        bail!("Plugin registry contains conflicting source {kind} '{name}'");
    }
    sources
        .entry(name.to_string())
        .or_insert_with(|| plugin_name.to_string());
    Ok(())
}

fn validate_options_schema(schema: &Value) -> Result<()> {
    jsonschema::validator_for(schema).context("Invalid plugin source options JSON Schema")?;
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    let valid = (1..=64).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    if !valid {
        bail!("Invalid plugin or source name '{name}'");
    }
    Ok(())
}

fn validate_target(target: &str) -> Result<()> {
    match target {
        "x86_64-unknown-linux-gnu"
        | "aarch64-unknown-linux-gnu"
        | "x86_64-pc-windows-msvc"
        | "x86_64-apple-darwin"
        | "aarch64-apple-darwin" => Ok(()),
        _ => bail!("Unsupported plugin target triple: {target}"),
    }
}

fn validate_trusted_keys(keys: &[TrustedKey]) -> Result<()> {
    let mut ids = HashSet::new();
    for key in keys {
        if key.key_id.is_empty() || !ids.insert(key.key_id.as_str()) {
            bail!("Trusted plugin registry key IDs must be non-empty and unique");
        }
        let public_key = decode_verifying_key(key)?;
        let expected_id = key_id(&public_key.to_bytes());
        if key.key_id != expected_id {
            bail!(
                "Trusted key ID '{}' does not match its public key fingerprint '{}'",
                key.key_id,
                expected_id
            );
        }
    }
    Ok(())
}

fn signing_seed_from_environment() -> Result<[u8; 32]> {
    let encoded =
        std::env::var(SIGNING_KEY_ENV).with_context(|| format!("{SIGNING_KEY_ENV} is not set"))?;
    let decoded = BASE64_STANDARD
        .decode(encoded)
        .context("Plugin registry signing key is not valid base64")?;
    decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("Plugin registry signing key must be a 32-byte Ed25519 seed"))
}

fn sign_bytes(bytes: &[u8], seed: &[u8; 32]) -> RegistrySignatureSet {
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key = signing_key.verifying_key();
    RegistrySignatureSet {
        schema_version: REGISTRY_SCHEMA_VERSION,
        signatures: vec![RegistrySignature {
            key_id: key_id(&verifying_key.to_bytes()),
            signature: BASE64_STANDARD.encode(signing_key.sign(bytes).to_bytes()),
        }],
    }
}

fn verify_bytes(bytes: &[u8], signature_bytes: &[u8], keys: &[TrustedKey]) -> Result<()> {
    let envelope: RegistrySignatureSet =
        serde_json::from_slice(signature_bytes).context("Invalid registry signature envelope")?;
    if envelope.schema_version != REGISTRY_SCHEMA_VERSION {
        bail!(
            "Unsupported registry signature schema {}",
            envelope.schema_version
        );
    }
    for signature in envelope.signatures {
        let Some(key) = keys
            .iter()
            .find(|key| key.key_id == signature.key_id && !key.revoked)
        else {
            continue;
        };
        let verifying_key = decode_verifying_key(key)?;
        let signature = BASE64_STANDARD
            .decode(signature.signature)
            .context("Invalid registry signature base64")?;
        let signature =
            Signature::from_slice(&signature).context("Invalid registry signature length")?;
        if verifying_key.verify(bytes, &signature).is_ok() {
            return Ok(());
        }
    }
    bail!("Plugin registry has no valid trusted signature")
}

fn decode_verifying_key(key: &TrustedKey) -> Result<VerifyingKey> {
    let bytes = BASE64_STANDARD
        .decode(&key.public_key)
        .with_context(|| format!("Invalid public key encoding for '{}'", key.key_id))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid public key length for '{}'", key.key_id))?;
    VerifyingKey::from_bytes(&bytes)
        .with_context(|| format!("Invalid Ed25519 public key '{}'", key.key_id))
}

fn key_id(public_key: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key);
    format!("ed25519-sha256:{}", hex_lower(&digest))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    fn index_bytes() -> Vec<u8> {
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "revision": 1,
            "generated_at": (Utc::now() - Duration::minutes(1)).to_rfc3339(),
            "expires_at": (Utc::now() + Duration::days(1)).to_rfc3339(),
            "plugins": []
        }))
        .unwrap()
    }

    fn trusted_key(seed: &[u8; 32]) -> TrustedKey {
        let public_key = SigningKey::from_bytes(seed).verifying_key().to_bytes();
        TrustedKey {
            key_id: key_id(&public_key),
            public_key: BASE64_STANDARD.encode(public_key),
            revoked: false,
        }
    }

    /// 検証: signature は parse 済み JSON ではなく exact registry bytes を対象にする
    /// 理由: publisher と client の意味解釈差で署名対象が変化しないようにするため
    /// リスク:署名後の whitespace/field order 変更を検出できない
    #[test]
    fn signs_and_verifies_exact_registry_bytes() {
        let seed = [7_u8; 32];
        let bytes = index_bytes();
        let signature = serde_json::to_vec(&sign_bytes(&bytes, &seed)).unwrap();
        let keys = vec![trusted_key(&seed)];

        verify_bytes(&bytes, &signature, &keys).unwrap();

        let mut modified = bytes;
        modified.push(b'\n');
        assert!(verify_bytes(&modified, &signature, &keys).is_err());
    }

    #[test]
    fn rejects_revoked_or_mismatched_trusted_keys() {
        let seed = [7_u8; 32];
        let other_seed = [8_u8; 32];
        let bytes = index_bytes();
        let signature = serde_json::to_vec(&sign_bytes(&bytes, &seed)).unwrap();

        assert!(verify_bytes(&bytes, &signature, &[trusted_key(&other_seed)]).is_err());
        let mut revoked = trusted_key(&seed);
        revoked.revoked = true;
        assert!(verify_bytes(&bytes, &signature, &[revoked]).is_err());
    }
}
