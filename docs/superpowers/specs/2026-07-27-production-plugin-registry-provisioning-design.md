# Production Plugin Registry Provisioning Design

## Goal

Provision the production Ed25519 trust root and protected GitHub publishing
environment for the optional MCP plugin registry. Publish and independently
verify an empty signed registry before starting the X connector tracked by
GitHub issue
[#85](https://github.com/rurusasu/article-collector/issues/85).

GitHub issue
[#84](https://github.com/rurusasu/article-collector/issues/84) owns this
provisioning, release verification, and cleanup phase. Initial provisioning
used `feat/84-registry-provision`; the independent refresh remediation uses
the dedicated `fix/84-registry-refresh` branch based on verified `main`.

## Scope

- Generate one production 32-byte Ed25519 seed outside the repository.
- Store the base64 seed in the administrator's password manager and the
  protected GitHub `plugin-registry` environment.
- Commit only the matching public key and key fingerprint.
- Enable the existing registry publisher after its protected environment is
  configured.
- Release a core binary containing the production trust key.
- Publish and independently verify an empty signed registry.
- Keep the signed registry fresh even when no core release occurs for more
  than 30 days.
- Close issue #84 and clean up its branches/worktrees only after the
  production verification succeeds.

## Out of Scope

- Implementing or publishing the X connector.
- Adding any production plugin entry to the registry.
- Supporting multiple active production signing keys.
- Changing registry schema v1, plugin protocol v1, or the registry endpoint.
- Introducing KMS, Vault, third-party registries, or user-configurable trust
  roots.
- Publishing repository registry content from the independent refresh path.

## Secret and Trust Boundary

The private material is a base64-encoded 32-byte Ed25519 seed accepted through
`PLUGIN_REGISTRY_SIGNING_KEY`. It must never be written to the repository,
workspace files, GitHub issues, pull requests, normal command output, or CI
logs.

The administrator generates the seed in a local interactive terminal, saves
the recovery copy directly in a password manager, and registers it as the
`PLUGIN_REGISTRY_SIGNING_KEY` secret in the GitHub `plugin-registry`
environment. The provisioning workflow must not ask Codex or another
non-interactive automation channel to display or relay the seed.

The repository stores only a `TrustedKey` entry containing:

- the base64 Ed25519 public key;
- `ed25519-sha256:<lowercase-hex>` over the raw public key bytes as `key_id`;
- `revoked: false`.

The `plugin-registry` environment permits registry publication only from the
main-branch Release workflow. Pull-request workflows validate public schemas
and workflow structure but never reference the private secret.

## Publication Flow

1. Configure the protected `plugin-registry` environment and its signing
   secret before enabling publication.
2. Add the matching public key to `registry/trusted-keys.json`.
3. Remove the publisher's fixed `if: ${{ false }}` guard without weakening its
   `needs: build`, environment, concurrency, or GitHub App token boundaries.
4. Validate the public trust set, registry schema, shell scripts, and workflow
   security in pull-request CI.
5. Merge and release a core version that embeds the production public key.
6. After all five core binary builds succeed, run the protected publisher.
7. Download any existing `plugin-registry-v1` assets and authenticate them
   before calculating the next revision.
8. Set the new revision to
   `max(source revision, published revision) + 1`, refresh timestamps, sign the
   exact JSON bytes, and verify the result before upload.
9. Upload `plugins-v1.json` and `plugins-v1.sig` together to the stable
   `plugin-registry-v1` GitHub Release.
10. Independently download the published assets and verify them with the
    committed trust set.

The initial production registry intentionally contains no plugins. Publishing
it proves the trust and release path independently of X-specific code.

## Independent Refresh Flow

The release publisher remains the only path that may publish registry content
from the repository. A separate `Plugin Registry Refresh` workflow keeps the
already-published content usable during periods without core releases:

1. Run every Monday at `03:17 UTC` and support an explicit
   `workflow_dispatch`.
2. Run only from the default branch and use the protected
   `plugin-registry` environment.
3. Share the `plugin-registry-publish` concurrency group with the release
   publisher and never cancel an in-progress publication.
4. Download `plugins-v1.json` and `plugins-v1.sig` from the stable
   `plugin-registry-v1` release.
5. Authenticate the exact downloaded bytes with the committed trust set.
   Expiry is ignored only for this authenticated recovery operation; schema,
   signing key, and signature checks remain mandatory.
6. Preserve the complete published registry payload and change only
   `revision`, `generated_at`, and `expires_at`.
7. Use the Rust registry utility to increment the authenticated `u64`
   revision with `checked_add(1)`, failing closed at `u64::MAX`.
8. Capture one current UTC instant, set it as `generated_at`, and derive
   `expires_at` as exactly 30 days after that same instant.
9. Sign the refreshed exact bytes using the protected environment secret,
   verify them again, and replace both stable release assets.

The weekly cadence leaves at least a 23-day buffer before the 30-day expiry.
If a scheduled run fails, GitHub Actions reports the failure and a maintainer
can use the same manual dispatch path after correcting the cause.

The refresh workflow must not read `registry/plugins-v1.json` as publication
input. Consequently, merging future plugin metadata into `main` cannot expose
that plugin before the corresponding core release publisher runs.

## Considered Alternatives

- Add a refresh mode to the existing Release workflow. This reuses more YAML,
  but couples release/build conditions to a recovery path and makes it easier
  to accidentally publish repository content without a core release.
- Extend the registry expiry substantially. This reduces immediate failures
  but does not provide continuous operation or an authenticated recovery
  mechanism.

The separate workflow is selected because its input boundary is explicit:
published authenticated content in, refreshed metadata out.

## Failure Handling

- A missing secret, invalid seed, public/private key mismatch, signing error,
  schema error, or signature verification error fails the publisher.
- Existing published assets are authenticated before their revision is
  trusted.
- No upload occurs until local signing and verification both succeed.
- The publisher never reuses or decreases a published revision.
- The independent refresh fails closed when the stable registry assets are
  missing, malformed, signed by an untrusted key, or fail signature
  verification.
- A refresh never falls back to repository registry content and never uploads
  before payload-preservation, signing, and exact-byte verification succeed.
- A failed publication leaves issue #84 open and retains its branches and
  worktrees for diagnosis.
- A failed core release does not run the registry publisher because the
  publisher continues to depend on the build matrix.

GitHub Release asset replacement is not transactional across two files. The
publisher therefore verifies both files before upload and uses one serialized
publisher job. Clients downloading during replacement may receive a temporary
pair mismatch, which fails closed under exact-byte signature verification and
can be retried after publication completes.

## Verification

Pull-request verification:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
task verify-pr PR_NUMBER=123
```

Production smoke verification:

- The core GitHub Release contains successful Linux amd64/arm64, macOS
  amd64/arm64, and Windows amd64 assets.
- `plugin-registry-v1` contains non-empty `plugins-v1.json` and
  `plugins-v1.sig` assets.
- The published JSON is fresh, schema v1, monotonically revised, and has an
  empty `plugins` array.
- The published signature verifies against the public key embedded in the
  released core.
- A manual independent refresh increments the authenticated revision, extends
  the expiry, preserves the non-metadata registry payload, and produces a
  signature that verifies against the production trust set.
- `article-collector plugin install x` reaches the authenticated registry and
  fails with an unknown-plugin error rather than an unprovisioned trust,
  download, expiry, or signature error.
- No private seed appears in the repository, workflow logs, release assets, or
  issue/PR content.

## Release and Cleanup

Issue #84 remains open until both the release publication smoke and the
independent manual refresh verification pass. After success:

1. record the core release, registry release, revision, and verification
   evidence in issue #84;
2. close issue #84;
3. remove the merged #84 branches and their linked worktrees;
4. create the issue #85 branch from the verified `main`;
5. publish X connector artifacts before adding the X entry to the signed
   registry.
