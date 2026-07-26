# Signed MCP Plugin Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a signed, curated, optionally installed MCP plugin system whose first supported capability is `recommend-source`.

**Architecture:** Keep plugin installation and execution outside the static site registry. A signed registry resolves platform executables, a versioned local store activates them atomically, a bounded MCP stdio client validates their runtime descriptor, and a plugin source adapter converts validated candidates into the existing recommendation pipeline.

**Tech Stack:** Rust stable, Tokio, reqwest/rustls, RMCP 2.2, serde/TOML/JSON, JSON Schema draft 2020-12, Ed25519, SHA-256, GitHub Releases/Actions.

## Global Constraints

- GitHub issue [#84](https://github.com/rurusasu/article-collector/issues/84) owns this plan; use branch `feat/84-plugin-foundation`.
- Do not implement X-specific behavior; GitHub issue #85 owns the X connector.
- Per the user's explicit direction, use GitHub issue #84 as the sole tracker; do not create or update Plane issues.
- Initial capability is exactly `recommend-source`.
- Initial registry is one compiled-in official endpoint; do not add URL overrides, local installs, or third-party registries.
- Plugin protocol is MCP over stdio; plugin stdout is protocol-only.
- The core owns `source`, `site`, and `rank`; plugins cannot set them.
- Support `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, and `aarch64-apple-darwin`.
- Use Rust stable and keep `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --release --locked` green.
- Follow RED → verify RED → GREEN → verify GREEN for every behavior change.
- Do not publish a registry signed with a development key. Production trust-key provisioning is a release prerequisite.

---

## File Structure

- Create `src/plugins/mod.rs`: public plugin subsystem facade and constants.
- Create `src/plugins/types.rs`: registry, installed-state, MCP request/response, and candidate types.
- Create `src/plugins/paths.rs`: OS data-root resolution, target mapping, name validation, and safe child paths.
- Create `src/plugins/state.rs`: installed-state reads, atomic activation, listing, and exact uninstall.
- Create `src/plugins/registry.rs`: bounded registry fetch, Ed25519 verification, expiry/rollback checks, and version resolution.
- Create `src/plugins/installer.rs`: artifact download/hash, staging, probe, install, and update transaction.
- Create `src/plugins/runtime.rs`: bounded MCP stdio transport, lifecycle timeouts, tool calls, and descriptor/candidate validation.
- Create `src/plugins/sources.rs`: installed plugin source catalog and recommendation adapter.
- Create `src/plugins/commands.rs`: CLI command orchestration and user-facing summaries.
- Modify `src/main.rs`: add the `plugin` command tree and pass the plugin catalog into recommendation collection.
- Modify `src/config.rs`: add plugin-owned `options` tables without weakening unknown-field checks elsewhere.
- Modify `src/recommend.rs`: resolve and execute built-in or plugin source plans.
- Modify `src/paths.rs`: expose the production plugin data root.
- Modify `Cargo.toml` and `Cargo.lock`: add the minimum direct dependencies required by the subsystem.
- Create `tests/plugin_cli.rs`: end-to-end CLI behavior against temporary signed registries and injected test services.
- Modify `tests/architecture.rs`: enforce focused plugin module boundaries.
- Create `registry/plugins-v1.json`: source metadata for the production registry publisher; initially no production plugins.
- Create `registry/trusted-keys.json`: public trust set, populated only from the provisioned production key.
- Create `examples/plugin_registry.rs`: schema validation, signing, and signature verification utility.
- Create `scripts/verify-plugin-registry.sh`: CI/release registry checks.
- Modify `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `scripts/verify-workflow-security.sh`, and `Taskfile.yml`: verify and publish signed registry assets without exposing the signing key to PR jobs.
- Create `docs/architectures/plugins.md`: protocol, trust, storage, and lifecycle documentation.
- Modify `docs/AGENTS.md`, `docs/architectures/files.md`, `docs/architectures/usage.md`, and `README.md`: index and user documentation.

### Stable Interfaces

```rust
pub const PLUGIN_PROTOCOL_VERSION: &str = "1.0.0";
pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

pub struct PluginPaths {
    pub data_root: PathBuf,
}

pub struct InstalledPluginStore {
    paths: PluginPaths,
}

pub struct RegistryClient {
    http: reqwest::Client,
    endpoint: Url,
    trusted_keys: Vec<TrustedKey>,
}

pub struct PluginInstaller<P> {
    registry: RegistryClient,
    store: InstalledPluginStore,
    probe: P,
}

pub trait PluginProbe {
    async fn verify(&self, plugin: &InstalledPlugin) -> anyhow::Result<PluginDescriptor>;
}

pub struct PluginRuntime;

pub struct PluginSourceCatalog {
    sources: BTreeMap<String, PluginSource>,
    aliases: BTreeMap<String, String>,
}
```

---

### Task 1: Define Versioned Plugin and Registry Contracts

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `src/plugins/mod.rs`
- Create: `src/plugins/types.rs`
- Modify: `src/main.rs`
- Modify: `src/config.rs`

**Interfaces:**
- Produces: `RegistryIndex`, `RegistrySignatureSet`, `PluginManifest`, `InstalledPlugin`, `PluginDescriptor`, `CollectRequest`, `CollectResponse`, and `PluginCandidate`.
- Produces: `RecommendSiteConfig.options: Option<toml::Table>`.
- Consumes: existing `serde`, `serde_json`, `toml`, `chrono`, `reqwest`, and `tokio`.

- [x] **Step 1: Add failing config and protocol serialization tests**

Add tests that prove plugin options remain typed data and contract versions are explicit:

```rust
#[test]
fn parses_plugin_source_options_without_accepting_unknown_top_level_keys() {
    let config = parse_config(
        r#"
        [recommend.source.example]
        limit = 10

        [recommend.source.example.options]
        mode = "timeline"
        include_reposts = false
        "#,
    )
    .unwrap();

    let options = config.recommend.source["example"].options.as_ref().unwrap();
    assert_eq!(options["mode"].as_str(), Some("timeline"));
    assert_eq!(options["include_reposts"].as_bool(), Some(false));
}

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
```

- [x] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked config::tests::parses_plugin_source_options_without_accepting_unknown_top_level_keys
cargo test --locked plugins::types::tests::registry_contract_rejects_unknown_fields
```

Expected: FAIL because `options`, the plugin module, and registry types do not exist.

- [x] **Step 3: Add direct dependencies and contract types**

Add these dependencies and regenerate `Cargo.lock` with `cargo check`:

```toml
rmcp = { version = "2.2.0", default-features = false, features = ["client", "transport-async-rw"] }
jsonschema = { version = "0.49.1", default-features = false }
ed25519-dalek = "3"
semver = { version = "1", features = ["serde"] }
sha2 = "0.11"
base64 = "0.23"
url = { version = "2", features = ["serde"] }
futures = "0.3"
tokio-util = { version = "0.7", features = ["codec"] }
tempfile = "3"
```

Create strict serde models in `src/plugins/types.rs`. Use `#[serde(deny_unknown_fields)]` on registry schema v1, installed-state v1, and the application MCP contract:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub revision: u64,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub plugins: Vec<RegistryPlugin>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryPluginVersion {
    pub version: Version,
    pub protocol: VersionReq,
    pub manifest: PluginManifest,
    pub artifacts: BTreeMap<String, RegistryArtifact>,
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
```

Define `PluginManifest` with `name`, `version`, `protocol`, `capabilities`,
`sources`, and `environment`; permit only `recommend-source` in validation,
not in deserialization.

Add `mod plugins;` to `src/main.rs`. Add `options: Option<toml::Table>` to
`RecommendSiteConfig` and change affected config derives from `Eq` to
`PartialEq` only if `toml::Value` prevents `Eq`.

- [x] **Step 4: Run GREEN tests and contract unit tests**

Run:

```sh
cargo test --locked config::tests plugins::types::tests
```

Expected: PASS.

- [x] **Step 5: Commit the contract**

```sh
git add Cargo.toml Cargo.lock src/main.rs src/config.rs src/plugins
git commit -m "feat: define plugin protocol and registry contracts"
```

---

### Task 2: Add Safe Plugin Paths and Atomic Installed State

**Files:**
- Create: `src/plugins/paths.rs`
- Create: `src/plugins/state.rs`
- Modify: `src/plugins/mod.rs`
- Modify: `src/paths.rs`

**Interfaces:**
- Consumes: `PluginManifest`, `InstalledPlugin`.
- Produces: `PluginPaths::from_data_root`, `PluginPaths::system`, `current_plugin_target`, `validate_plugin_name`.
- Produces: `InstalledPluginStore::{list,get,activate,uninstall}`.

- [x] **Step 1: Write failing path, target, activation, and symlink tests**

Add table-driven tests for all five targets and state behavior:

```rust
#[test]
fn maps_supported_os_arch_pairs_to_release_targets() {
    assert_eq!(plugin_target("linux", "x86_64").unwrap(), "x86_64-unknown-linux-gnu");
    assert_eq!(plugin_target("linux", "aarch64").unwrap(), "aarch64-unknown-linux-gnu");
    assert_eq!(plugin_target("windows", "x86_64").unwrap(), "x86_64-pc-windows-msvc");
    assert_eq!(plugin_target("macos", "x86_64").unwrap(), "x86_64-apple-darwin");
    assert_eq!(plugin_target("macos", "aarch64").unwrap(), "aarch64-apple-darwin");
    assert!(plugin_target("windows", "aarch64").is_err());
}

#[test]
fn failed_activation_keeps_previous_installed_state() {
    let temp = tempfile::tempdir().unwrap();
    let store = fixture_store(temp.path());
    store.activate(&fixture_installed("example", "1.0.0")).unwrap();
    let result = store.activate_with_writer(&fixture_installed("example", "1.1.0"), |_| {
        Err(std::io::Error::other("injected failure"))
    });
    assert!(result.is_err());
    assert_eq!(store.get("example").unwrap().unwrap().version, Version::parse("1.0.0").unwrap());
}
```

On Unix, add a test that inserts a symlink in the plugin path and expects
`ensure_safe_plugin_path` to reject it. On Windows, add a junction/reparse-point
metadata rejection test behind `#[cfg(windows)]`.

- [x] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked plugins::paths::tests
cargo test --locked plugins::state::tests
```

Expected: FAIL because the modules and store do not exist.

- [x] **Step 3: Implement deterministic paths and state transactions**

Use `dirs::data_local_dir()` with `article-collector` appended. Do not reuse the
temporary artifact directory. Validate plugin names with
`^[a-z0-9][a-z0-9-]{0,63}$`.

Implement activation as:

```rust
pub fn activate(&self, installed: &InstalledPlugin) -> Result<()> {
    let plugin_root = self.paths.plugin_root(&installed.name)?;
    fs::create_dir_all(&plugin_root)?;
    let temp = NamedTempFile::new_in(&plugin_root)?;
    serde_json::to_writer_pretty(temp.as_file(), installed)?;
    temp.as_file().sync_all()?;
    temp.persist(plugin_root.join("installed.json"))?;
    sync_parent(&plugin_root)?;
    Ok(())
}
```

Before reads, writes, or deletes, walk `symlink_metadata` from the plugin data
root to the target and reject symlinks/reparse points. `uninstall` accepts only
a validated installed name, removes its exact directory, and never accepts an
arbitrary path.

- [x] **Step 4: Run GREEN tests**

Run:

```sh
cargo test --locked plugins::paths::tests
cargo test --locked plugins::state::tests
cargo test --locked paths::tests
```

Expected: PASS.

- [x] **Step 5: Commit path and state support**

```sh
git add src/paths.rs src/plugins
git commit -m "feat: add safe plugin state storage"
```

---

### Task 3: Verify and Resolve the Signed Registry

**Files:**
- Create: `src/plugins/registry.rs`
- Modify: `src/plugins/mod.rs`
- Create: `registry/plugins-v1.json`
- Create: `registry/trusted-keys.json`

**Interfaces:**
- Consumes: `RegistryIndex`, `RegistrySignatureSet`, `TrustedKey`, target string, current protocol version.
- Produces: `verify_registry`, `resolve_plugin`, `RegistryClient::fetch`.

- [x] **Step 1: Write failing cryptographic and resolver tests**

Generate an Ed25519 fixture key in test code with a constant test seed. Sign the
exact registry bytes and cover success plus every rejection:

```rust
#[test]
fn verifies_signed_registry_and_selects_latest_compatible_version() {
    let fixture = signed_registry_fixture([
        version("1.0.0", ">=1.0.0, <2.0.0"),
        version("2.0.0", ">=2.0.0, <3.0.0"),
    ]);
    let index = verify_registry(
        &fixture.index_bytes,
        &fixture.signature_bytes,
        &[fixture.trusted_key()],
        utc("2026-07-26T12:00:00Z"),
        Some(6),
    )
    .unwrap();
    let resolved = resolve_plugin(&index, "example", None, "aarch64-apple-darwin").unwrap();
    assert_eq!(resolved.version, Version::parse("1.0.0").unwrap());
}
```

Separate tests must assert the error category for: bad signature, unknown key,
expired metadata, lower cached revision, wrong schema version, duplicate plugin
name/version, unsupported target, incompatible protocol, and missing pinned
version.

- [x] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked plugins::registry::tests
```

Expected: FAIL because registry verification and resolution do not exist.

- [x] **Step 3: Implement exact-byte Ed25519 verification and strict resolution**

Parse the small signature envelope first, decode each base64 signature, and
verify the original `plugins-v1.json` bytes:

```rust
pub fn verify_registry(
    index_bytes: &[u8],
    signature_bytes: &[u8],
    trusted_keys: &[TrustedKey],
    now: DateTime<Utc>,
    cached_revision: Option<u64>,
) -> Result<RegistryIndex> {
    let signatures: RegistrySignatureSet = serde_json::from_slice(signature_bytes)?;
    verify_any_signature(index_bytes, &signatures, trusted_keys)?;
    let index: RegistryIndex = serde_json::from_slice(index_bytes)?;
    validate_registry(&index, now, cached_revision)?;
    Ok(index)
}
```

Reject duplicate semantic entries explicitly after deserialization. Sort
compatible versions by `semver::Version`, select the greatest stable version,
and require an exact match for `name@version`.

Implement bounded downloads: registry JSON maximum 2 MiB, signature maximum
64 KiB, HTTPS only, connect timeout 10 seconds, total timeout 30 seconds.

Commit an empty schema-v1 source index with a finite generated/expiry window.
Keep `trusted-keys.json` empty until the production public key is provisioned;
official install/update commands must fail with a clear message while the
embedded trust set is empty.

- [x] **Step 4: Run GREEN tests**

Run:

```sh
cargo test --locked plugins::registry::tests
```

Expected: PASS.

- [x] **Step 5: Commit registry verification**

```sh
git add src/plugins registry
git commit -m "feat: verify signed plugin registry"
```

---

### Task 4: Implement Transactional Install, Update, and Uninstall

**Files:**
- Create: `src/plugins/installer.rs`
- Modify: `src/plugins/state.rs`
- Modify: `src/plugins/mod.rs`

**Interfaces:**
- Consumes: `RegistryClient`, `InstalledPluginStore`, `PluginProbe`.
- Produces: `PluginInstaller::{install,update,uninstall}` and `InstallOutcome`.

- [x] **Step 1: Write failing transaction tests with a local HTTP server**

Use a local Tokio TCP fixture server that returns signed registry bytes and
artifact bytes. Inject a probe that returns a matching descriptor:

```rust
#[tokio::test]
async fn update_hashes_probes_and_atomically_switches_active_version() {
    let fixture = InstallerFixture::new().await;
    fixture.install_version("1.0.0").await.unwrap();
    fixture.publish_version("1.1.0", b"new executable").await;

    let outcome = fixture.installer.update("example").await.unwrap();

    assert_eq!(outcome.version, Version::parse("1.1.0").unwrap());
    assert_eq!(fixture.store.get("example").unwrap().unwrap().version, outcome.version);
    assert!(!fixture.paths.any_staging_directory());
}
```

Add focused tests for wrong length, wrong SHA-256, interrupted response,
non-HTTPS production artifact URL, probe mismatch, activation failure, pinned
version behavior, already-current no-op, and preservation of the old active
version.

- [x] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked plugins::installer::tests
```

Expected: FAIL because the installer does not exist.

- [x] **Step 3: Implement bounded streaming download and staging**

Stream the response into a `NamedTempFile` in the destination filesystem while
incrementally updating `Sha256` and counting bytes. Reject the transfer as soon
as it exceeds the signed length.

The install order is:

```rust
let resolved = self.registry.resolve(request).await?;
let staging = self.store.create_staging(&resolved)?;
self.download_and_verify(&resolved.artifact, staging.executable()).await?;
staging.make_executable()?;
let descriptor = self.probe.verify(&staging.as_installed_plugin()).await?;
validate_descriptor(&resolved.manifest, &descriptor)?;
let installed = staging.commit_version_directory()?;
self.store.activate(&installed)?;
Ok(InstallOutcome::Installed(installed))
```

If any operation before `activate` fails, drop/cleanup staging and leave
`installed.json` unchanged. Update reuses install with an active-version
precondition. Uninstall delegates to the exact-name store operation.

- [x] **Step 4: Run GREEN tests**

Run:

```sh
cargo test --locked plugins::installer::tests
cargo test --locked plugins::state::tests
```

Expected: PASS.

- [x] **Step 5: Commit installer transactions**

```sh
git add src/plugins
git commit -m "feat: install plugins transactionally"
```

---

### Task 5: Add a Bounded MCP Plugin Runtime

**Files:**
- Create: `src/plugins/runtime.rs`
- Modify: `src/plugins/types.rs`
- Modify: `src/plugins/mod.rs`

**Interfaces:**
- Implements: `PluginProbe for PluginRuntime`.
- Produces: `PluginRuntime::{describe,configure,collect,doctor}`.
- Consumes: absolute installed executable and signed environment allowlist.

- [x] **Step 1: Read the shared good-test rules and write failing runtime tests**

Read `superpowers/test-driven-development/writing-good-tests.md` before editing
tests. Use RMCP in-memory duplex transports for successful tool behavior and
platform-specific test commands only for crash/hang behavior.

Required tests:

```rust
#[tokio::test]
async fn collect_rejects_reserved_metadata_and_oversized_frames() {
    let runtime = runtime_with_fake_server(FakeBehavior::Collect(json!({
        "items": [{
            "title": "candidate",
            "url": "https://example.com",
            "content": "body",
            "metadata": {"source": "spoofed"}
        }]
    })));
    let error = runtime.collect(&fixture_plugin(), fixture_request()).await.unwrap_err();
    assert!(error.to_string().contains("reserved metadata field 'source'"));
}
```

Also cover missing required tools, descriptor mismatch, non-structured tool
output, invalid URL, too many items, 8 KiB title, 1 MiB content, 16 MiB frame,
10-second initialize timeout, 60-second collect timeout, 300-second configure
timeout through paused Tokio time, abnormal exit, and 256 KiB stderr truncation.

- [x] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked plugins::runtime::tests
```

Expected: FAIL because the MCP runtime does not exist.

- [x] **Step 3: Implement a max-frame transport and lifecycle**

Do not use RMCP's default `AsyncRwTransport`, whose line limit is unbounded.
Implement `BoundedChildTransport` with
`JsonRpcMessageCodec::new_with_max_length(16 * 1024 * 1024)`,
`FramedRead`, and `FramedWrite`. Own the Tokio child in the transport so close
waits 5 seconds and then kills it.

Build `tokio::process::Command` from the canonical executable path:

```rust
let mut command = Command::new(&plugin.executable);
command
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .env_clear();
copy_allowed_environment(&mut command, &plugin.manifest.environment)?;
```

After MCP initialize, call `tools/list` and require the two mandatory tool
names. Require `structured_content` for application responses. Deserialize into
strict contract types, then perform semantic validation and return typed
candidates.

Capture stderr concurrently into a fixed 256 KiB buffer. Include only the
sanitized tail in errors; redact values for allowed secret-like variable names
and never include MCP request payloads.

- [x] **Step 4: Run GREEN tests**

Run:

```sh
cargo test --locked plugins::runtime::tests
```

Expected: PASS with no leaked child processes.

- [x] **Step 5: Commit MCP runtime**

```sh
git add src/plugins
git commit -m "feat: run plugins over bounded MCP stdio"
```

---

### Task 6: Integrate Plugin Sources with Recommendation Collection

**Files:**
- Create: `src/plugins/sources.rs`
- Modify: `src/plugins/mod.rs`
- Modify: `src/recommend.rs`
- Modify: `src/config.rs`
- Modify: `src/sites/mod.rs`

**Interfaces:**
- Produces: `PluginSourceCatalog::{load,resolve,recommendable_sources}`.
- Produces: `PluginSourceCollector::collect`.
- Modifies: `RecommendationTarget` and `SourcePlan` to represent built-in or plugin backends.

- [ ] **Step 1: Write failing source resolution and pipeline tests**

Add tests proving installed plugin sources are dynamic and isolated from static
site routing:

```rust
#[tokio::test]
async fn explicit_plugin_source_enters_existing_history_pipeline() {
    let catalog = fixture_catalog_with_source("example", &["ex"]);
    let collector = fixture_plugin_collector(vec![candidate("https://example.com/1")]);
    let config = plugin_config("example", json!({"mode":"latest"}));

    let collection = collect_recommended_with_plugins(
        "ex", Some(1), None, &config, &logger(), &catalog, &collector
    )
    .await
    .unwrap();

    assert_eq!(collection.item_count, 1);
    let raw: Value = serde_json::from_slice(&fs::read(collection.raw_path).unwrap()).unwrap();
    assert_eq!(raw[0]["source"], "example");
    assert_eq!(raw[0]["site"], "example");
    assert_eq!(raw[0]["rank"], 1);
}
```

Add tests for built-in/plugin collision, alias collision, plugin query override
rejection, built-in `options` rejection, configured `all` ordering, default
built-ins-then-sorted-plugins ordering, disabled plugin source, explicit plugin
failure, and `all` partial failure.

- [ ] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked recommend::tests::explicit_plugin_source_enters_existing_history_pipeline plugins::sources::tests
```

Expected: FAIL because recommendation targets cannot represent plugin sources.

- [ ] **Step 3: Implement the dynamic source backend**

Change the internal target and plan shape:

```rust
enum RecommendationTarget<'a> {
    AllSources,
    BuiltIn { site_name: &'static str, endpoint: DiscoveryEndpoint },
    Plugin { source: PluginSource },
    PageLinks { url: &'a str },
}

enum SourceBackend {
    BuiltIn { site_name: &'static str, endpoint: DiscoveryEndpoint },
    Plugin(PluginSource),
}

struct SourcePlan {
    name: String,
    backend: SourceBackend,
    limit: usize,
    query: Option<String>,
    options: Option<toml::Table>,
}
```

Load installed manifests without launching plugins. Validate catalog collisions
against `sites::SITES`. For plugin plans, reject CLI `--query`, validate
`options` with the source JSON Schema, call the plugin collector, and convert:

```rust
fn candidate_to_recommendation(source: &str, rank: usize, candidate: PluginCandidate) -> Value {
    let mut object = candidate.metadata;
    object.insert("source".into(), json!(source));
    object.insert("site".into(), json!(source));
    object.insert("rank".into(), json!(rank));
    object.insert("title".into(), json!(candidate.title));
    object.insert("url".into(), json!(candidate.url));
    object.insert("content".into(), json!(candidate.content));
    Value::Object(object)
}
```

Keep direct URL routing and static site docs unchanged in this phase.

- [ ] **Step 4: Run GREEN and recommendation regression tests**

Run:

```sh
cargo test --locked plugins::sources::tests recommend::tests sites::tests
```

Expected: PASS, including existing Twitter direct API tests until #85 removes
that backend.

- [ ] **Step 5: Commit recommendation integration**

```sh
git add src/config.rs src/recommend.rs src/sites src/plugins
git commit -m "feat: collect recommendations from plugins"
```

---

### Task 7: Add Plugin CLI Commands

**Files:**
- Create: `src/plugins/commands.rs`
- Modify: `src/plugins/mod.rs`
- Modify: `src/main.rs`
- Create: `tests/plugin_cli.rs`

**Interfaces:**
- Produces CLI subcommands: `install`, `list`, `update`, `configure`, `doctor`, `uninstall`.
- Consumes `PluginInstaller`, `InstalledPluginStore`, and `PluginRuntime`.

- [ ] **Step 1: Write failing CLI parsing and outcome tests**

Add tests that use temporary data roots through an injected command context,
not a production registry URL override:

```rust
#[test]
fn root_help_lists_plugin_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_article-collector"))
        .args(["plugin", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["install", "list", "update", "configure", "doctor", "uninstall"] {
        assert!(stdout.contains(command), "missing {command}: {stdout}");
    }
}

#[test]
fn install_reference_accepts_name_and_exact_version() {
    assert_eq!(PluginReference::parse("example").unwrap().version, None);
    assert_eq!(
        PluginReference::parse("example@1.2.3").unwrap().version,
        Some(Version::parse("1.2.3").unwrap())
    );
    assert!(PluginReference::parse("../example").is_err());
}
```

Cover unknown plugin, unsupported configure, doctor descriptor failure,
already-current update, deterministic list order, and explicit uninstall
summary.

- [ ] **Step 2: Run RED tests**

Run:

```sh
cargo test --locked --test plugin_cli
```

Expected: FAIL because the command tree does not exist.

- [ ] **Step 3: Implement the command tree and orchestration**

Add Clap enums:

```rust
#[derive(Subcommand)]
enum PluginCommands {
    Install { plugin: PluginReference },
    List,
    Update { name: String },
    Configure { name: String },
    Doctor { name: String },
    Uninstall { name: String },
}
```

Keep constructors in `plugins::commands::production_context()` so tests can
inject a fixture endpoint, trust key, data root, probe, and clock without
introducing user-facing environment overrides.

Output one concise result per operation. Uninstall must state the exact removed
name/version and that the application cannot recover it except by reinstalling.

- [ ] **Step 4: Run GREEN CLI tests**

Run:

```sh
cargo test --locked --test plugin_cli
cargo test --locked --test cli root_help
```

Expected: PASS.

- [ ] **Step 5: Commit CLI commands**

```sh
git add src/main.rs src/plugins tests/plugin_cli.rs
git commit -m "feat: add plugin management commands"
```

---

### Task 8: Add Registry Publishing and Workflow Guards

**Files:**
- Create: `examples/plugin_registry.rs`
- Create: `scripts/verify-plugin-registry.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `scripts/verify-workflow-security.sh`
- Modify: `Taskfile.yml`
- Modify: `registry/plugins-v1.json`
- Modify: `registry/trusted-keys.json`

**Interfaces:**
- Produces: `plugin_registry validate`, `sign`, and `verify` commands.
- Produces: PR-safe schema verification and protected post-merge registry publishing.

- [ ] **Step 1: Write failing publisher and workflow-security checks**

Add Rust tests for exact-byte signatures and shell checks requiring:

```sh
require_contains "$release" "PLUGIN_REGISTRY_SIGNING_KEY" "registry signing uses the protected secret"
require_contains "$release" "environment: plugin-registry" "registry signing is protected by an Actions environment"
require_not_contains "$ci" "PLUGIN_REGISTRY_SIGNING_KEY" "PR CI cannot access the registry signing key"
require_contains "$release" "plugin-registry-v1" "registry assets use the stable release tag"
```

Add `scripts/verify-plugin-registry.sh` initially calling a missing example so
the test fails for the expected reason.

- [ ] **Step 2: Run RED checks**

Run:

```sh
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
```

Expected: FAIL because the publisher and workflow rules are absent.

- [ ] **Step 3: Implement schema validation, protected signing, and upload**

`examples/plugin_registry.rs` must:

- `validate <index> <trusted-keys>` without private material;
- `sign <index> <signature-output>` using base64
  `PLUGIN_REGISTRY_SIGNING_KEY` from the process environment;
- `verify <index> <signature> <trusted-keys>`;
- never print the secret or derived private key.

PR CI runs only `validate` and fixture verification. The release/scheduled job
uses `environment: plugin-registry`, reads the secret, increments revision,
refreshes expiry, signs exact bytes, verifies locally, and uploads both assets
to the `plugin-registry-v1` release with `gh release upload --clobber`.

Do not enable the publishing job until a repository administrator has:

1. generated the Ed25519 key outside logs;
2. committed its public key with a stable key ID;
3. stored the base64 private seed as `PLUGIN_REGISTRY_SIGNING_KEY`;
4. protected the `plugin-registry` Actions environment.

Update workflow hardening checks so a missing environment, secret reference in
PR jobs, unpinned action, or excessive token permission fails CI.

- [ ] **Step 4: Run GREEN workflow and registry checks**

Run:

```sh
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
shellcheck scripts/*.sh
```

Expected: PASS without accessing the production private key.

- [ ] **Step 5: Commit registry operations**

```sh
git add examples registry scripts Taskfile.yml .github/workflows
git commit -m "ci: publish signed plugin registry"
```

---

### Task 9: Document, Verify, and Prepare the Branch

**Files:**
- Create: `docs/architectures/plugins.md`
- Modify: `docs/AGENTS.md`
- Modify: `docs/architectures/files.md`
- Modify: `docs/architectures/usage.md`
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-07-26-signed-mcp-plugin-foundation-design.md`
- Modify: `docs/superpowers/plans/2026-07-26-signed-mcp-plugin-foundation.md`

**Interfaces:**
- Documents plugin protocol v1, registry schema v1, trust boundaries, commands, storage, errors, signing rotation, release ordering, and cleanup.

- [ ] **Step 1: Add failing documentation architecture assertions**

Extend `tests/architecture.rs`:

```rust
#[test]
fn plugin_architecture_is_documented_and_indexed() {
    let root = manifest_root();
    let plugin_doc = root.join("docs/architectures/plugins.md");
    assert!(plugin_doc.exists());
    let index = fs::read_to_string(root.join("docs/AGENTS.md")).unwrap();
    assert!(index.contains("architectures/plugins.md"));
}
```

- [ ] **Step 2: Run RED documentation test**

Run:

```sh
cargo test --locked --test architecture plugin_architecture_is_documented_and_indexed
```

Expected: FAIL because the architecture document is absent.

- [ ] **Step 3: Write user and maintainer documentation**

Document:

- install/list/update/configure/doctor/uninstall examples;
- official-registry-only behavior;
- OS data paths and no-recovery uninstall message;
- process permissions and lack of a complete sandbox;
- protocol tools and candidate reserved fields;
- registry exact-byte signing, expiry, rollback protection, and key rotation;
- artifact-first then registry release order;
- staging cleanup and branch/worktree cleanup;
- #85 as the first production plugin follow-up.

Keep the design's work-tracking section aligned with GitHub issue #84 as the
sole tracker and do not add Plane references.

- [ ] **Step 4: Run full verification and inspect the diff**

Run:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
taplo format --check Cargo.toml article-collector.toml release-plz.toml
taplo lint Cargo.toml article-collector.toml release-plz.toml
shellcheck scripts/*.sh
bash scripts/verify-workflow-security.sh
bash scripts/verify-plugin-registry.sh
git diff --check
git status --short
git diff --stat main...HEAD
```

Expected: every command PASS; only #84 files are changed.

- [ ] **Step 5: Commit documentation and verification**

```sh
git add README.md docs tests/architecture.rs
git commit -m "docs: document MCP plugin lifecycle"
```

- [ ] **Step 6: Update tracking and prepare handoff**

Update GitHub #84 to reflect the actual implementation, verification,
release-secret readiness, and cleanup state. Do not mark complete while
registry signing-key provisioning or release verification remains incomplete.

After all checks pass, use `superpowers:requesting-code-review`, then
`superpowers:verification-before-completion`, and finally
`superpowers:finishing-a-development-branch`. Push/open a PR only through the
approved GitHub publish workflow. Before updating PR checkboxes, run:

```sh
task verify-pr PR_NUMBER=<actual-pr-number>
```

Manually verify and report every SKIP before changing the PR description.

---

## Plan Self-Review

- Spec coverage: plugin protocol, signed registry, local state, install/update,
  runtime bounds, recommendation integration, CLI, release, docs, and cleanup
  each map to a task.
- Placeholder scan: no implementation placeholder remains; the only external
  prerequisite is explicit production signing-key provisioning.
- Type consistency: `PluginManifest`, `InstalledPlugin`, `PluginDescriptor`,
  `PluginProbe`, `PluginRuntime`, `PluginSourceCatalog`, and
  `PluginInstaller<P>` use the same names and ownership across tasks.
- Scope: X-specific OAuth, X MCP tool mapping, and removal of the current X
  direct collector remain exclusively in #85.
