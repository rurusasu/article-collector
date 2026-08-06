# Production Plugin Registry Provisioning Implementation Plan

> **Status (2026-08-06):** trust root、publisher、production verification は完了。
> empty registry の smoke test は過去の受け入れ条件であり、現在は X plugin が公開済み。
> 現行仕様は [../../architectures/plugins.md](../../architectures/plugins.md) を参照する。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provision the production Ed25519 trust root, enable the protected registry publisher, and prove the released core can authenticate an empty production plugin registry.

**Architecture:** Extend the existing registry utility with a one-time stdin-based public trust-set derivation command so the private seed never enters the repository or command arguments. Commit the derived public key, enable the existing build-gated publisher, and verify the exact published bytes from an isolated Docker smoke environment before closing GitHub issue #84.

**Tech Stack:** Rust stable, `ed25519-dalek`, GitHub Actions environments/secrets, GitHub CLI, Bash, Docker, GitHub Releases.

## Global Constraints

- GitHub issue [#84](https://github.com/rurusasu/article-collector/issues/84) is the sole tracker; do not update Plane.
- Work on `feat/84-registry-provision`, based on `origin/main`.
- The private material is one base64-encoded 32-byte Ed25519 seed.
- Never place the private seed in repository files, workspace files, command arguments, command output, logs, issues, pull requests, or release assets.
- Store the recovery copy in the administrator's password manager and the GitHub `plugin-registry` environment secret `PLUGIN_REGISTRY_SIGNING_KEY`.
- Commit only the matching public key, an `ed25519-sha256:` key ID followed by 64 lowercase hexadecimal characters, and `revoked: false`.
- PR workflows must not reference `PLUGIN_REGISTRY_SIGNING_KEY`.
- The publisher remains dependent on all five core binary builds and uses the serialized `plugin-registry-publish` concurrency group.
- The initial production registry has an empty `plugins` array.
- Do not implement the X connector or remove the existing direct X collector in this plan.
- Use Rust stable and keep `Cargo.lock` unchanged unless Cargo itself requires an update.
- Resolve the PR number from GitHub, then run `task verify-pr PR_NUMBER="$pr_number"` before checking any PR checklist boxes.

---

### Task 1: Add a Safe Initial Trust-Set Derivation Command

**Files:**
- Modify: `examples/plugin_registry.rs`
- Test: `examples/plugin_registry.rs`

**Interfaces:**
- Consumes: a base64-encoded 32-byte Ed25519 seed from stdin.
- Produces: `Commands::ProvisionTrustSet { trusted_keys_output: PathBuf }`.
- Produces: `signing_seed_from_base64(encoded: &str) -> anyhow::Result<[u8; 32]>`.
- Produces: `trusted_key_from_seed(seed: &[u8; 32]) -> TrustedKey`.
- Produces: `write_initial_trust_set(path: &Path, seed: &[u8; 32]) -> anyhow::Result<()>`.
- Invariant: `write_initial_trust_set` accepts only an existing empty trust-set or a missing output file and refuses to replace a non-empty trust-set.

- [ ] **Step 1: Add tests for seed parsing, public-key derivation, and overwrite refusal**

Replace the test-only `trusted_key` helper with the production
`trusted_key_from_seed` helper and add:

```rust
#[test]
fn derives_trusted_key_from_base64_seed() {
    let seed = [7_u8; 32];
    let encoded = BASE64_STANDARD.encode(seed);

    let parsed = signing_seed_from_base64(&encoded).unwrap();
    let trusted = trusted_key_from_seed(&parsed);
    let signed = serde_json::to_vec(&sign_bytes(b"registry", &parsed)).unwrap();

    assert_eq!(parsed, seed);
    assert!(!trusted.revoked);
    assert_eq!(
        trusted.key_id,
        key_id(&SigningKey::from_bytes(&seed).verifying_key().to_bytes())
    );
    verify_bytes(b"registry", &signed, &[trusted]).unwrap();
}

#[test]
fn initial_trust_set_refuses_to_replace_existing_key() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("trusted-keys.json");

    write_initial_trust_set(&path, &[7_u8; 32]).unwrap();
    let first = fs::read(&path).unwrap();

    let error = write_initial_trust_set(&path, &[8_u8; 32]).unwrap_err();
    assert!(error.to_string().contains("already contains a key"));
    assert_eq!(fs::read(&path).unwrap(), first);
}
```

- [ ] **Step 2: Run the focused tests and confirm RED**

Run:

```bash
cargo test --locked --example plugin_registry derives_trusted_key_from_base64_seed
cargo test --locked --example plugin_registry initial_trust_set_refuses_to_replace_existing_key
```

Expected: both commands fail to compile because
`signing_seed_from_base64`, `trusted_key_from_seed`, and
`write_initial_trust_set` do not exist.

- [ ] **Step 3: Add the one-time provisioning subcommand**

Add imports:

```rust
use std::io::{self, Read};
```

Add the command variant:

```rust
/// Derive an initial public trust set from a base64 Ed25519 seed read from stdin
ProvisionTrustSet {
    trusted_keys_output: PathBuf,
},
```

Handle it in `main`:

```rust
Commands::ProvisionTrustSet {
    trusted_keys_output,
} => {
    let mut encoded = String::new();
    io::stdin()
        .take(128)
        .read_to_string(&mut encoded)
        .context("Failed to read the plugin registry signing seed from stdin")?;
    let seed = signing_seed_from_base64(encoded.trim())?;
    write_initial_trust_set(&trusted_keys_output, &seed)?;
    println!(
        "Wrote initial public plugin registry trust set -> {}",
        trusted_keys_output.display()
    );
}
```

Refactor environment parsing and add the public-key helpers:

```rust
fn signing_seed_from_environment() -> Result<[u8; 32]> {
    let encoded =
        std::env::var(SIGNING_KEY_ENV).with_context(|| format!("{SIGNING_KEY_ENV} is not set"))?;
    signing_seed_from_base64(&encoded)
}

fn signing_seed_from_base64(encoded: &str) -> Result<[u8; 32]> {
    let decoded = BASE64_STANDARD
        .decode(encoded)
        .context("Plugin registry signing key is not valid base64")?;
    decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("Plugin registry signing key must be a 32-byte Ed25519 seed"))
}

fn trusted_key_from_seed(seed: &[u8; 32]) -> TrustedKey {
    let public_key = SigningKey::from_bytes(seed).verifying_key().to_bytes();
    TrustedKey {
        key_id: key_id(&public_key),
        public_key: BASE64_STANDARD.encode(public_key),
        revoked: false,
    }
}

fn write_initial_trust_set(path: &Path, seed: &[u8; 32]) -> Result<()> {
    if path.exists() {
        let current = read_trusted_keys(path)?;
        if !current.is_empty() {
            bail!(
                "Trusted key output {} already contains a key",
                path.display()
            );
        }
    }
    write_json(path, &vec![trusted_key_from_seed(seed)])
}
```

Update existing tests to call `trusted_key_from_seed` instead of their local
`trusted_key` helper.

- [ ] **Step 4: Run focused and registry tests and confirm GREEN**

Run:

```bash
cargo fmt --check
cargo test --locked --example plugin_registry
bash scripts/verify-plugin-registry.sh
```

Expected: formatting passes, all `plugin_registry` example tests pass, and the
registry validation script passes while `registry/trusted-keys.json` is still
empty.

- [ ] **Step 5: Commit the utility**

```bash
git add examples/plugin_registry.rs
git commit -m "feat: derive initial plugin registry trust set"
```

### Task 2: Provision the Protected Environment and Commit the Public Trust Key

**Files:**
- Modify: `registry/trusted-keys.json`

**Interfaces:**
- Consumes: the administrator's clipboard containing a newly generated
  base64-encoded 32-byte seed.
- Produces: GitHub environment `plugin-registry`.
- Produces: environment secret `PLUGIN_REGISTRY_SIGNING_KEY`.
- Produces: one non-revoked `TrustedKey` in `registry/trusted-keys.json`.
- Security checkpoint: these commands are run by the administrator in their
  local interactive terminal, not through a tool invocation that captures
  stdout.

- [ ] **Step 1: Create the protected GitHub environment**

Run with the authenticated GitHub CLI:

```bash
env -u GH_TOKEN gh api \
  --method PUT \
  repos/rurusasu/article-collector/environments/plugin-registry \
  --input - <<'JSON'
{
  "deployment_branch_policy": {
    "protected_branches": false,
    "custom_branch_policies": true
  }
}
JSON

env -u GH_TOKEN gh api \
  --method POST \
  repos/rurusasu/article-collector/environments/plugin-registry/deployment-branch-policies \
  -f name=main \
  -f type=branch
```

If the `main` policy already exists, confirm it with:

```bash
env -u GH_TOKEN gh api \
  repos/rurusasu/article-collector/environments/plugin-registry/deployment-branch-policies
```

Expected: the environment exists and its only custom deployment branch policy
is `main`.

- [ ] **Step 2: Generate the production seed without terminal output**

The administrator runs this in a local macOS terminal:

```zsh
umask 077
openssl rand 32 | base64 | tr -d '\n' | pbcopy
```

Immediately paste the clipboard value into the password manager as the
recovery copy. Do not paste it into chat, an issue, a file, or a command
argument.

- [ ] **Step 3: Register the protected Actions secret from stdin**

With the seed still in the clipboard, run:

```zsh
pbpaste | env -u GH_TOKEN gh secret set \
  PLUGIN_REGISTRY_SIGNING_KEY \
  --env plugin-registry \
  --repo rurusasu/article-collector
```

Confirm the secret name without reading its value:

```bash
env -u GH_TOKEN gh secret list \
  --env plugin-registry \
  --repo rurusasu/article-collector |
  awk '$1 == "PLUGIN_REGISTRY_SIGNING_KEY" { found = 1 } END { exit !found }'
```

Expected: the check exits zero and does not print the secret value.

- [ ] **Step 4: Derive and validate the public trust set from stdin**

Run from the repository worktree:

```zsh
pbpaste | cargo run --quiet --locked --example plugin_registry -- \
  provision-trust-set registry/trusted-keys.json
printf '' | pbcopy
```

Then run:

```bash
cargo run --quiet --locked --example plugin_registry -- \
  validate registry/plugins-v1.json registry/trusted-keys.json
jq -e '
  length == 1 and
  .[0].revoked == false and
  (.[0].key_id | test("^ed25519-sha256:[0-9a-f]{64}$")) and
  (.[0].public_key | type == "string" and length > 0)
' registry/trusted-keys.json
git diff --check
```

Expected: registry validation succeeds, exactly one non-revoked public key is
present, and no private seed is printed.

- [ ] **Step 5: Commit the public trust key**

```bash
git add registry/trusted-keys.json
git commit -m "chore: provision plugin registry trust key"
```

### Task 3: Enable the Build-Gated Publisher and Update Operations Documentation

**Files:**
- Modify: `scripts/verify-workflow-security.sh`
- Modify: `scripts/verify-rules.sh`
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`
- Modify: `docs/architectures/plugins.md`
- Modify: `docs/ci-cd/README.md`

**Interfaces:**
- Consumes: the protected `plugin-registry` environment and committed public
  trust key from Task 2.
- Produces: an enabled `plugin-registry-publish` job that runs only after the
  successful five-target `build` matrix.
- Produces: a security guard that rejects reintroduction of a fixed-disabled
  publisher.
- Produces: a PR checklist rule that runs the public registry verification
  without accessing the production secret.

- [ ] **Step 1: Change the workflow security assertion first**

Replace the fixed-disabled assertion with:

```bash
require_not_contains "$release" "if: \${{ false }}" "registry publishing is enabled after production key provisioning"
require_contains "$release" "needs: build" "registry publishing waits for the complete release build matrix"
```

Add this rule to `VERIFY_RULES` in `scripts/verify-rules.sh`:

```bash
"plugin registry schema|registry schema.*公開鍵|公開鍵検証:::bash scripts/verify-plugin-registry.sh:::Verify plugin registry schema and public trust set"
```

- [ ] **Step 2: Run the guard and confirm RED**

Run:

```bash
bash scripts/verify-workflow-security.sh
```

Expected: non-zero exit with
`registry publishing is enabled after production key provisioning` failing
because `.github/workflows/release.yml` still contains `if: ${{ false }}`.

- [ ] **Step 3: Enable the publisher without changing its trust boundaries**

In `.github/workflows/release.yml`, remove:

```yaml
# Disabled until the production public key, protected secret, and environment are provisioned.
if: ${{ false }}
```

Keep all of the following unchanged:

```yaml
needs: build
concurrency:
  group: plugin-registry-publish
  cancel-in-progress: false
environment: plugin-registry
```

Do not add `always()` or any condition that would run after a failed or
skipped build matrix.

- [ ] **Step 4: Update operator and user documentation**

Update `README.md` so it says the production trust root is provisioned and the
authenticated registry is available, but currently contains no installable
plugins until #85 is released.

Update `docs/architectures/plugins.md` with:

- the one-time `provision-trust-set` command reading the seed from stdin;
- the `main` deployment branch restriction;
- the password-manager and environment-secret recovery boundary;
- the fact that publication is enabled only after successful core builds;
- the empty-registry smoke-test expectation.

Update `docs/ci-cd/README.md` so the Release diagram and security section show
the protected registry publisher after the build matrix.

- [ ] **Step 5: Run focused verification and confirm GREEN**

Run:

```bash
cargo fmt --check
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
if command -v shellcheck >/dev/null 2>&1; then
  shellcheck -x scripts/*.sh
else
  nix shell nixpkgs#shellcheck -c shellcheck -x scripts/*.sh
fi
git diff --check
```

Expected: every command exits zero. The workflow security output explicitly
passes both the enabled-publisher and `needs: build` assertions.

- [ ] **Step 6: Commit publisher enablement and documentation**

```bash
git add \
  .github/workflows/release.yml \
  scripts/verify-workflow-security.sh \
  scripts/verify-rules.sh \
  README.md \
  docs/architectures/plugins.md \
  docs/ci-cd/README.md
git commit -m "ci: enable production plugin registry publishing"
```

### Task 4: Complete Local Verification and Open the Provisioning PR

**Files:**
- Verify only: all files changed by Tasks 1-3 and both provisioning docs.

**Interfaces:**
- Consumes: a clean `feat/84-registry-provision` branch with the protected
  environment already configured.
- Produces: a reviewed GitHub PR linked to issue #84 with a verified checklist.

- [ ] **Step 1: Run the complete local verification**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
if command -v taplo >/dev/null 2>&1; then
  taplo format --check Cargo.toml article-collector.toml release-plz.toml
  taplo lint Cargo.toml article-collector.toml release-plz.toml
else
  nix shell nixpkgs#taplo -c taplo format --check Cargo.toml article-collector.toml release-plz.toml
  nix shell nixpkgs#taplo -c taplo lint Cargo.toml article-collector.toml release-plz.toml
fi
if command -v shellcheck >/dev/null 2>&1; then
  shellcheck -x scripts/*.sh
else
  nix shell nixpkgs#shellcheck -c shellcheck -x scripts/*.sh
fi
git diff --check
git status --short
```

Expected: all commands exit zero and `git status --short` is empty.

- [ ] **Step 2: Review the branch diff**

Run:

```bash
git diff --stat origin/main...HEAD
git diff origin/main...HEAD -- \
  examples/plugin_registry.rs \
  registry/trusted-keys.json \
  .github/workflows/release.yml \
  scripts/verify-workflow-security.sh \
  scripts/verify-rules.sh \
  README.md \
  docs/architectures/plugins.md \
  docs/ci-cd/README.md \
  docs/superpowers/specs/2026-07-27-production-plugin-registry-provisioning-design.md \
  docs/superpowers/plans/2026-07-27-production-plugin-registry-provisioning.md
```

Confirm that no private seed, unexpected dependency, X connector code, or
unrelated source change is present.

- [ ] **Step 3: Push and open a draft PR**

Use the repository publishing workflow:

```bash
env -u GH_TOKEN git push -u origin feat/84-registry-provision
```

Create a draft PR titled
`ci: provision production plugin registry signing` with `Refs #84` in the
body and these unchecked items:

```markdown
- [ ] cargo fmt、clippy、test、release buildが成功する
- [ ] plugin registry schemaと公開鍵検証が成功する
- [ ] GitHub Actions workflow security検証が成功する
- [ ] shellcheckとTOML検証が成功する
- [ ] production secretがPR・repository・通常ログに含まれない
- [ ] release後に署名済みempty registryをproduction検証する
```

- [ ] **Step 4: Run the mandatory PR checklist verification**

Resolve the PR number and run:

```bash
pr_number="$(
  env -u GH_TOKEN gh pr view \
    --repo rurusasu/article-collector \
    --json number \
    --jq .number
)"
env -u GH_TOKEN task verify-pr PR_NUMBER="$pr_number"
```

Expected: all automatically mapped items PASS. The secret-absence and
production-smoke items may be SKIP because they require manual evidence.
Confirm secret absence from the branch diff and report that manual result.
Leave the production-smoke item unchecked and explicitly report it as blocked
until the release exists.

- [ ] **Step 5: Complete review and CI**

Request an independent code review, address all Critical and Important
findings, rerun the affected checks, and push fixes as focused commits.
Monitor GitHub Actions until all required checks pass. Mark only the
pre-release checklist items whose evidence is complete, then move the PR from
draft to ready for review.

Record the PR URL, head SHA, local verification, checklist result, CI state,
and remaining production smoke step in issue #84.

### Task 5: Merge, Release, Verify Production, and Clean Up

**Files:**
- No source edits expected.
- External state: feature PR, release-plz PR, GitHub Releases, issue #84,
  branches, and linked worktree.

**Interfaces:**
- Consumes: approved provisioning PR with all pre-release checks passing.
- Produces: a released core containing the public trust key.
- Produces: authenticated `plugin-registry-v1/plugins-v1.json` and
  `plugins-v1.sig`.
- Produces: closed issue #84 and removed #84 branches/worktrees only after the
  smoke test passes.

- [ ] **Step 1: Merge the provisioning PR and monitor main CI**

Merge only when the PR is mergeable, required checks pass, and no unresolved
review threads remain. Record the merge commit and monitor its Release
workflow. The feature merge should update or create a release-plz PR; it
should not publish a registry because no new core release is created yet.

- [ ] **Step 2: Verify and merge the release-plz PR**

Inspect the release PR and confirm it changes only `Cargo.toml` and
`Cargo.lock` version metadata. Run:

```bash
release_pr="$(
  env -u GH_TOKEN gh pr list \
    --repo rurusasu/article-collector \
    --state open \
    --search 'head:release-plz-' \
    --json number \
    --jq '.[0].number'
)"
env -u GH_TOKEN task verify-pr PR_NUMBER="$release_pr"
```

Require all release PR checks to pass before merge. Merge it and record the
merge commit and resulting version tag.

- [ ] **Step 3: Monitor the release and registry publisher**

Watch the Release workflow until completion:

```bash
run_id="$(
  env -u GH_TOKEN gh run list \
    --repo rurusasu/article-collector \
    --workflow Release \
    --branch main \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId'
)"
env -u GH_TOKEN gh run watch \
  --repo rurusasu/article-collector \
  "$run_id" \
  --exit-status
```

Expected:

- GitHub Release creation succeeds.
- Linux amd64/arm64, macOS amd64/arm64, and Windows amd64 builds/uploads
  succeed.
- `Publish Plugin Registry` succeeds rather than skipping.

- [ ] **Step 4: Independently verify published registry assets**

Use a fresh temporary directory:

```bash
registry_smoke_dir="$(mktemp -d)"
env -u GH_TOKEN gh release download plugin-registry-v1 \
  --repo rurusasu/article-collector \
  --pattern plugins-v1.json \
  --pattern plugins-v1.sig \
  --dir "$registry_smoke_dir"

cargo run --quiet --locked --example plugin_registry -- \
  verify \
  "$registry_smoke_dir/plugins-v1.json" \
  "$registry_smoke_dir/plugins-v1.sig" \
  registry/trusted-keys.json

jq -e '
  .schema_version == 1 and
  .revision >= 2 and
  (.plugins | type == "array" and length == 0) and
  (.generated_at | fromdateiso8601 <= now) and
  (.expires_at | fromdateiso8601 > now)
' "$registry_smoke_dir/plugins-v1.json"
```

Expected: exact-byte signature verification passes and the registry is fresh,
monotonically revised, and empty.

- [ ] **Step 5: Prove the released core reports unknown plugin after trust verification**

Resolve the new core release tag and download the Linux amd64 binary:

```bash
core_tag="$(
  env -u GH_TOKEN gh release list \
    --repo rurusasu/article-collector \
    --exclude-drafts \
    --exclude-pre-releases \
    --limit 1 \
    --json tagName \
    --jq '.[0].tagName'
)"
core_smoke_dir="$(mktemp -d)"
env -u GH_TOKEN gh release download "$core_tag" \
  --repo rurusasu/article-collector \
  --pattern article-collector-linux-amd64 \
  --dir "$core_smoke_dir"
chmod +x "$core_smoke_dir/article-collector-linux-amd64"
```

Run the binary in an ephemeral Linux container so registry state does not
touch the administrator's normal user data:

```bash
set +e
install_output="$(
  docker run --rm \
    --mount "type=bind,src=$core_smoke_dir,dst=/work,readonly" \
    ubuntu:24.04 \
    /work/article-collector-linux-amd64 plugin install x 2>&1
)"
install_status=$?
set -e

test "$install_status" -ne 0
printf '%s\n' "$install_output" | grep -F "Unknown plugin 'x'"
```

Expected: the command fails only because `x` is absent. It must not report
unprovisioned trust keys, download failure, expiry, or invalid signature.

- [ ] **Step 6: Record evidence, close #84, and clean up**

Add an issue #84 comment containing:

- provisioning PR and merge commit;
- released core tag and five asset names;
- Release workflow URL and successful publisher job;
- registry release URL, revision, expiry, and signature verification result;
- isolated `plugin install x` unknown-plugin result;
- confirmation that no secret value was exposed.

Mark the final PR production-smoke checklist item only after rerunning
`task verify-pr` and manually recording its evidence. Close issue #84 as
completed.

From the repository's primary checkout, confirm both branches are merged into
`origin/main`, then remove only the exact #84 targets:

```bash
git -C /Users/ktome1995/Program/article-collector fetch origin main
git -C /Users/ktome1995/Program/article-collector \
  merge-base --is-ancestor feat/84-plugin-foundation origin/main
git -C /Users/ktome1995/Program/article-collector \
  merge-base --is-ancestor feat/84-registry-provision origin/main
git -C /Users/ktome1995/Program/article-collector \
  worktree remove /Users/ktome1995/orca/workspaces/article-collector/feat-xapi-mcp-connection
git -C /Users/ktome1995/Program/article-collector branch -d feat/84-plugin-foundation
git -C /Users/ktome1995/Program/article-collector branch -d feat/84-registry-provision

for branch in feat/84-plugin-foundation feat/84-registry-provision; do
  if env -u GH_TOKEN git -C /Users/ktome1995/Program/article-collector \
    ls-remote --exit-code --heads origin "refs/heads/$branch" >/dev/null 2>&1; then
    env -u GH_TOKEN git -C /Users/ktome1995/Program/article-collector \
      push origin --delete "$branch"
  fi
done
```

Do not run cleanup if either ancestry check fails or any production smoke
check is incomplete. After cleanup, create the issue #85 branch from the
verified `origin/main`.
