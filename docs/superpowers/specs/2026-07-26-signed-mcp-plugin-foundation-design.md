# Signed MCP Plugin Foundation Design

## Goal

Add an optional plugin system to `article-collector` so users can install
officially curated recommendation sources after installing the core binary.
The foundation owns discovery, installation, verification, lifecycle, and MCP
transport. Its first and only implemented capability is `recommend-source`.

GitHub issue
[#84](https://github.com/rurusasu/article-collector/issues/84) tracks this
phase. The X connector is a separate phase tracked by
[#85](https://github.com/rurusasu/article-collector/issues/85).

## Scope

- Install plugins by name from one official signed registry.
- Support Linux amd64/arm64, Windows amd64, and macOS amd64/arm64.
- Run plugins as external executables over MCP stdio.
- Add plugin-provided recommendation sources to direct and `all` collection.
- Validate plugin manifests, runtime capabilities, source configuration, and
  collected candidates before they reach the existing pipeline.
- Provide install, list, update, configure, doctor, and uninstall commands.
- Document plugin protocol v1, registry schema v1, signing, release, and
  cleanup procedures.

## Out of Scope

- The X connector or any other production plugin.
- Third-party registries, arbitrary artifact URLs supplied by users, or local
  development installs.
- Capabilities other than `recommend-source`.
- In-process dynamic libraries or a stable Rust plugin ABI.
- WebAssembly plugins.
- A complete OS sandbox. Process separation limits crashes but does not revoke
  the installed executable's user permissions.
- Streaming recommendation candidates.

## Architecture

The core process is an MCP client. Each installed plugin is a separate MCP
stdio server:

```text
article-collector
  -> installed manifest/source index
  -> validated plugin executable
  -> MCP initialize over stdio
  -> article_collector_describe_sources
  -> article_collector_collect
  -> validated candidates
  -> existing history/translation/save pipeline
```

The core uses manifests for lazy source resolution and does not launch every
installed plugin on startup. A selected plugin is launched only for an
explicit source, an enabled source in `recommend all`, `plugin configure`, or
`plugin doctor`.

Built-in sources, installed plugin source names, and all aliases share one
namespace. Duplicate names or aliases make the conflicting plugin unusable and
produce an actionable error. The core never silently changes source ownership.

## Plugin Protocol v1

Plugins use the standard MCP initialization and tool-call lifecycle over
stdio. Stdout is reserved for MCP frames. Stderr is a bounded diagnostics
stream and must not contain secrets.

### `article_collector_describe_sources`

This required tool takes no source-specific input and returns:

- plugin protocol version;
- plugin name and version;
- one or more provided source names;
- aliases for each source;
- a JSON Schema for each source's `options`;
- whether interactive configuration is supported.

The runtime response must match the signed installed manifest. A mismatch in
plugin identity, version, protocol, source, alias, or configuration support is
an integrity error.

### `article_collector_collect`

This required tool consumes:

```json
{
  "source": "example",
  "limit": 10,
  "options": {
    "mode": "example"
  }
}
```

It returns candidates in this application contract:

```json
{
  "items": [
    {
      "title": "Candidate title",
      "url": "https://example.com/article",
      "content": "Candidate content",
      "metadata": {
        "author": "Example"
      }
    }
  ]
}
```

Plugins cannot set the reserved `source`, `site`, or `rank` fields. The core
sets those fields from the resolved source and response order. The core rejects
non-HTTP(S) URLs, more items than requested, malformed metadata, titles over
8 KiB, individual content values over 1 MiB, or a complete tool response over
16 MiB.

### `article_collector_configure`

This optional tool performs interactive setup such as OAuth. The plugin owns
its credentials and persistence. It must not return access tokens, refresh
tokens, client secrets, or authorization codes to the core.

The CLI reports a clear unsupported error when this tool is absent.

## Configuration

Plugin-specific source configuration is isolated under `options`:

```toml
[recommend.source.example]
limit = 10

[recommend.source.example.options]
mode = "example"
```

The core converts the TOML table to JSON and validates it against the selected
source's JSON Schema before calling `collect`. Built-in sources reject
`options`; plugin sources reject built-in-only top-level settings that their
contract does not consume.

The source limit follows the existing precedence rules. The CLI limit overrides
the source limit, which overrides the global limit. The validated effective
limit is passed to the plugin.

## Official Signed Registry

The first release trusts one compiled-in registry endpoint. Users cannot
override it with a URL or add another registry.

The registry is published as two assets on the GitHub release tag
`plugin-registry-v1`:

- `plugins-v1.json`: exact signed registry bytes;
- `plugins-v1.sig`: key identifiers and Ed25519 signatures over those bytes.

Registry schema v1 contains:

- schema version;
- monotonically increasing revision;
- generated and expiry timestamps;
- plugin name and available versions;
- plugin protocol compatibility;
- source/capability manifest;
- target-specific executable URL, byte length, and SHA-256 digest.

The binary contains an Ed25519 public-key trust set. A registry update is
accepted only when at least one non-revoked trusted key verifies it. Key
rotation uses an overlap release: ship a core version that trusts old and new
keys, publish a registry signed by both, and retire the old key only after the
overlap version is established.

The cached registry revision cannot decrease. An expired registry cannot be
used for new installs or updates. Installed plugins continue to run while the
registry is temporarily unavailable or expired.

The registry authorizes artifact URLs. Artifacts still require HTTPS, an exact
byte length, and the signed SHA-256 digest. Plugins are distributed as raw
platform executables rather than archives, avoiding archive path traversal and
extraction ambiguity.

## Installation and Local State

Plugin state uses the OS user data directory:

```text
<user-data>/article-collector/
  registry/
    state.json
  plugins/
    <plugin-name>/
      installed.json
      <version>/
        plugin.toml
        <executable>[.exe]
```

Install and update use a staging directory under the destination plugin's
parent filesystem. The core downloads and verifies the registry, resolves an
exact compatible version and target, downloads the executable, verifies byte
length and SHA-256, writes the signed manifest subset, and runs a non-interactive
MCP handshake. Only then does it rename the version directory into place and
atomically replace `installed.json`.

An interrupted or failed operation leaves the active version unchanged.
Staging paths have process-specific unpredictable names and are removed on
success or handled failure. `plugin doctor` reports stale staging paths; a
subsequent successful install/update may remove stale paths owned by the same
plugin.

The commands are:

```text
article-collector plugin install <name>
article-collector plugin install <name>@<version>
article-collector plugin list
article-collector plugin update <name>
article-collector plugin configure <name>
article-collector plugin doctor <name>
article-collector plugin uninstall <name>
```

An unversioned install selects the latest registry version compatible with the
core protocol and current target. An explicit version never silently selects a
different version. Update uses the same resolver and no-ops when the active
version is current.

Uninstall removes only the exact plugin directory resolved from installed
state. It never follows symlinks and reports that the removed plugin is not
recoverable through the application, although the same registry version can be
installed again while available.

## Process and Trust Boundaries

- The executable path is canonicalized under the plugin data root and rejected
  if any path component is a symlink.
- The core starts it directly with an argument vector, never through a shell.
- The child receives a minimal environment: platform basics plus environment
  variable names explicitly declared in the signed manifest. Secret values are
  never copied into logs.
- MCP initialization has a 10-second timeout.
- Non-interactive `collect` calls have a 60-second timeout.
- Interactive `configure` calls have a 300-second timeout.
- Graceful shutdown has a 5-second timeout before the child is terminated.
- Stderr diagnostics are capped at 256 KiB per invocation.
- The core validates all application-level responses even after MCP schema
  validation.

Official registry curation and signatures establish publisher trust. They do
not make a plugin harmless, so documentation must state that an installed
plugin executes with the user's operating-system permissions.

## Error Handling

Installation fails without changing active state for:

- registry fetch, signature, expiry, or rollback failure;
- unknown plugin or version;
- unsupported platform;
- incompatible protocol;
- artifact redirect to non-HTTPS;
- byte-length or checksum mismatch;
- unsafe local path;
- MCP handshake failure;
- manifest/runtime descriptor mismatch.

Runtime errors identify the plugin, source, lifecycle phase, and cause without
including secrets. An explicitly requested plugin source fails the command.
During `recommend all`, a plugin source failure is recorded with the existing
source-failure mechanism and other enabled sources continue.

## Testing

Tests use a generated Ed25519 fixture key, signed local registry metadata, a
local HTTP fixture server, and fake MCP plugin executables. They do not depend
on the production registry or a real third-party account.

Coverage includes:

- target and compatible-version resolution for all five release targets;
- valid install, pinned install, update, configure, doctor, and uninstall;
- invalid signature, checksum, expiry, revision, target, and protocol;
- interrupted downloads and atomic update preservation;
- stale staging cleanup reporting;
- unsafe paths and symlinks;
- source and alias collisions;
- MCP timeout, crash, malformed frame, oversized response, and stderr cap;
- manifest/runtime descriptor mismatch;
- JSON Schema options validation;
- candidate validation and reserved-field ownership;
- direct plugin source collection and `recommend all` partial failure;
- OS-specific user data and executable path resolution.

Repository verification remains:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

## Release and Operations

The release workflow must verify registry schema and signatures but cannot
expose the signing secret to pull-request jobs. The Ed25519 private key is held
as a protected GitHub Actions secret available only to the registry publishing
environment. The protected publishing job refreshes the signed expiry,
increments the revision, and uploads both registry assets together.

Publishing a plugin follows this order:

1. build and test every declared target artifact;
2. publish immutable versioned artifacts;
3. calculate artifact byte lengths and SHA-256 digests;
4. update and sign the registry with a higher revision;
5. verify the uploaded registry and artifacts from a clean environment.

The issue branch is `feat/84-plugin-foundation`. It is removed after merge and
worktree cleanup. The X connector branch is created later from the main branch
that contains this foundation.

## Work Tracking

Per the user's explicit direction, GitHub issue #84 is the sole tracker for
this foundation. Its body records the purpose, scope, acceptance criteria,
verification, release work, and cleanup. No secondary tracker is used.

## Implementation Status

The generic foundation is implemented on `feat/84-plugin-foundation`.
Review hardening adds a startup-wide MCP deadline, process-group/Job Object
cleanup, bounded redacted diagnostics, canonical runtime path invariants,
deferred executable readiness, HTTPS-only redirects, client-side options
schema validation, and monotonic revision derivation from the published
registry.
Production registry publishing remains deliberately disabled until an
administrator provisions the Ed25519 public key, protected
`PLUGIN_REGISTRY_SIGNING_KEY` secret, and protected `plugin-registry`
environment. The X-specific binary, OAuth flow, timeline mapping, and removal
of the current direct X collector remain in GitHub #85.
