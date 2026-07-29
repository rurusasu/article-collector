# Independent Plugin Registry Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a protected weekly and manual workflow that refreshes the authenticated published plugin registry without waiting for a core release or exposing repository registry content.

**Architecture:** A dedicated GitHub Actions workflow downloads the stable `plugin-registry-v1` assets, authenticates their exact bytes, changes only revision and timestamps through a checked-in jq filter, signs and verifies the result in the protected environment, and replaces the stable assets. It shares the release publisher's concurrency group, while the existing release publisher remains the only path that may source plugin content from `registry/plugins-v1.json`.

**Tech Stack:** GitHub Actions, Bash, jq, Rust stable, the existing `plugin_registry` Rust example, GitHub CLI, Task.

## Global Constraints

- Work on `fix/84-registry-refresh`, based on verified `origin/main`, in the existing linked worktree.
- GitHub issue #84 owns this remediation. Do not create or update a Plane issue.
- Run every Monday at `03:17 UTC` and support `workflow_dispatch`.
- Run only from the default branch in the protected `plugin-registry` environment.
- Share `plugin-registry-publish` concurrency with the release publisher and set `cancel-in-progress: false`.
- Authenticate the downloaded exact bytes before trusting the published revision or content.
- Expiry may be ignored only by `verify-published`; schema, trust-key, and signature verification remain mandatory.
- Preserve every published registry field except `revision`, `generated_at`, and `expires_at`.
- Set revision to the authenticated published revision plus one and expiry to 30 days after generation.
- Never read `registry/plugins-v1.json` as the independent refresh publication input.
- Keep `PLUGIN_REGISTRY_SIGNING_KEY` scoped to the sign-and-verify step and out of PR workflows and logs.
- Never retrieve, display, search for, or write the production signing seed.
- Rust uses the stable toolchain. Required local checks are `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`.
- Before updating PR checklist boxes, run `task verify-pr PR_NUMBER=<number>` and manually verify every SKIP.
- Keep issue #84 open until the manual refresh production smoke and final independent review pass.

---

### Task 1: Implement and verify the independent refresh workflow

**Files:**
- Create: `.github/workflows/plugin-registry-refresh.yml`
- Create: `scripts/refresh-plugin-registry.jq`
- Modify: `scripts/verify-workflow-security.sh`
- Modify: `docs/architectures/plugins.md`
- Modify: `docs/ci-cd/README.md`

**Interfaces:**
- Consumes: stable release assets `plugin-registry-v1/plugins-v1.json` and `plugin-registry-v1/plugins-v1.sig`; `registry/trusted-keys.json`; protected secret `PLUGIN_REGISTRY_SIGNING_KEY`.
- Produces: refreshed `plugins-v1.json` and `plugins-v1.sig` on the same stable release; workflow name `Plugin Registry Refresh`; jq filter `scripts/refresh-plugin-registry.jq`.

- [ ] **Step 1: Add failing executable workflow-security tests**

Extend `scripts/verify-workflow-security.sh` before creating either production file. Define:

```bash
refresh=".github/workflows/plugin-registry-refresh.yml"
refresh_filter="scripts/refresh-plugin-registry.jq"
```

Add guards that fail when the workflow or filter is absent and require:

```bash
require_contains "$refresh" 'cron: "17 3 * * 1"' "registry refresh runs weekly before the 30-day expiry"
require_contains "$refresh" "workflow_dispatch:" "registry refresh supports manual recovery"
require_contains "$refresh" "github.event.repository.default_branch" "registry refresh is limited to the default branch"
require_contains "$refresh" "environment: plugin-registry" "registry refresh uses the protected environment"
require_contains "$refresh" "group: plugin-registry-publish" "registry refresh shares publisher serialization"
require_contains "$refresh" "cancel-in-progress: false" "registry refresh never cancels an active publisher"
require_contains "$refresh" "verify-published" "registry refresh authenticates published bytes before reuse"
require_contains "$refresh" "jq -er" "registry refresh applies the checked-in metadata filter"
require_contains "$refresh" "-f $refresh_filter" "registry refresh uses the tested metadata filter"
require_contains "$refresh" "gh release upload \"\$registry_tag\"" "registry refresh replaces stable release assets"
require_not_contains "$refresh" "registry/plugins-v1.json" "registry refresh never publishes repository registry content"
require_occurrences "$refresh" "PLUGIN_REGISTRY_SIGNING_KEY" 1 "registry refresh exposes the signing key to one step only"
```

Add a fixture containing a non-empty plugin entry. Execute the production jq filter with fixed arguments:

```bash
jq -er \
  --argjson revision 8 \
  --arg generated_at "2026-07-29T03:17:00Z" \
  --arg expires_at "2026-08-28T03:17:00Z" \
  -f "$refresh_filter" \
  <<< "$refresh_fixture"
```

Assert:

```jq
.revision == 8
and .generated_at == "2026-07-29T03:17:00Z"
and .expires_at == "2026-08-28T03:17:00Z"
```

Then compare the input and output after:

```jq
del(.revision, .generated_at, .expires_at)
```

They must be semantically equal, including the complete non-empty `plugins` entry.

- [ ] **Step 2: Run the new tests and record RED**

Run:

```bash
bash scripts/verify-workflow-security.sh
```

Expected: non-zero exit caused by the missing refresh workflow/filter and the new refresh requirements, while the pre-existing release security checks remain green.

- [ ] **Step 3: Add the minimal metadata filter**

Create `scripts/refresh-plugin-registry.jq`:

```jq
if (.revision | type) != "number"
  or .revision < 0
  or .revision != (.revision | floor)
then
  error("Published registry revision must be a non-negative integer")
else
  .revision = $revision
  | .generated_at = $generated_at
  | .expires_at = $expires_at
end
```

The workflow computes `$revision` from the authenticated input as `current + 1`; the filter does not accept plugin content from another source.

- [ ] **Step 4: Add the protected refresh workflow**

Create `.github/workflows/plugin-registry-refresh.yml` with:

```yaml
name: Plugin Registry Refresh

on:
  schedule:
    - cron: "17 3 * * 1"
  workflow_dispatch:

permissions:
  contents: read

jobs:
  refresh:
    name: Refresh Plugin Registry
    if: ${{ github.repository_owner == 'rurusasu' && github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
    runs-on: ubuntu-latest
    environment: plugin-registry
    concurrency:
      group: plugin-registry-publish
      cancel-in-progress: false
```

Use the same pinned `actions/checkout` and `actions/create-github-app-token` SHAs as `.github/workflows/release.yml`, with `persist-credentials: false` and `permission-contents: write`.

Add four ordered steps after checkout, Rust setup, and token creation:

1. **Download and authenticate published registry** without the signing secret:

```bash
set -euo pipefail
registry_tag="plugin-registry-v1"
mkdir published-registry
gh release download "$registry_tag" \
  --pattern plugins-v1.json \
  --pattern plugins-v1.sig \
  --dir published-registry
cargo run --locked --example plugin_registry -- \
  verify-published \
  published-registry/plugins-v1.json \
  published-registry/plugins-v1.sig \
  registry/trusted-keys.json
```

2. **Refresh metadata and prove payload preservation** without the signing secret:

```bash
set -euo pipefail
current_revision="$(
  jq -er '.revision | select(type == "number" and . >= 0 and . == floor)' \
    published-registry/plugins-v1.json
)"
next_revision=$((current_revision + 1))
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
expires_at="$(date -u -d '+30 days' +%Y-%m-%dT%H:%M:%SZ)"
jq -er \
  --argjson revision "$next_revision" \
  --arg generated_at "$generated_at" \
  --arg expires_at "$expires_at" \
  -f scripts/refresh-plugin-registry.jq \
  published-registry/plugins-v1.json > plugins-v1.json
jq -e -s '
  (.[0] | del(.revision, .generated_at, .expires_at))
  ==
  (.[1] | del(.revision, .generated_at, .expires_at))
' published-registry/plugins-v1.json plugins-v1.json
cargo run --locked --example plugin_registry -- \
  validate plugins-v1.json registry/trusted-keys.json
```

3. **Sign and exact-byte verify**, with the only workflow reference to:

```yaml
env:
  PLUGIN_REGISTRY_SIGNING_KEY: ${{ secrets.PLUGIN_REGISTRY_SIGNING_KEY }}
```

Run `plugin_registry sign` and then `plugin_registry verify`.

4. **Upload refreshed assets** without the signing secret:

```bash
gh release upload "$registry_tag" \
  plugins-v1.json \
  plugins-v1.sig \
  --clobber
```

Do not create the stable release when it is absent; download must fail closed.

- [ ] **Step 5: Run focused GREEN verification**

Run:

```bash
bash scripts/verify-workflow-security.sh
bash scripts/verify-plugin-registry.sh
if command -v shellcheck >/dev/null 2>&1; then
  shellcheck -x scripts/*.sh
else
  nix shell nixpkgs#shellcheck -c shellcheck -x scripts/*.sh
fi
git diff --check
```

Expected: all checks pass. The fixture proves the production filter preserves non-metadata payload.

- [ ] **Step 6: Document operation and failure handling**

Update `docs/architectures/plugins.md` and `docs/ci-cd/README.md` with:

- weekly Monday `03:17 UTC` and manual triggers;
- published authenticated assets as the only refresh input;
- shared protected environment and concurrency group;
- expiry-tolerant authentication for recovery;
- only revision/timestamps change;
- repository registry content remains release-only;
- zero/missing/invalid assets and signature failures fail closed;
- manual dispatch verification procedure.

Add the workflow to the CI/CD workflow file table and diagram.

- [ ] **Step 7: Run Task 1 validation and commit**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
if command -v taplo >/dev/null 2>&1; then
  taplo format --check Cargo.toml article-collector.toml release-plz.toml
  taplo lint Cargo.toml article-collector.toml release-plz.toml
fi
if command -v shellcheck >/dev/null 2>&1; then
  shellcheck -x scripts/*.sh
else
  nix shell nixpkgs#shellcheck -c shellcheck -x scripts/*.sh
fi
git diff --check
```

Expected: 276 tests pass, 2 live/configured tests remain ignored, and every static/security check passes.

Commit:

```bash
git add \
  .github/workflows/plugin-registry-refresh.yml \
  scripts/refresh-plugin-registry.jq \
  scripts/verify-workflow-security.sh \
  docs/architectures/plugins.md \
  docs/ci-cd/README.md
git commit -m "fix: keep plugin registry fresh between releases"
```

---

### Task 2: Publish and merge the remediation PR

**Files:**
- No new source files.
- Inspect: the complete `origin/main...HEAD` diff.
- External state: GitHub issue #84, remediation PR, CI runs, review threads.

**Interfaces:**
- Consumes: Task 1 reviewed commit and the committed design/plan.
- Produces: merged remediation PR on `main`, with all checklist and review gates satisfied.

- [ ] **Step 1: Run the complete pre-PR verification**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
taplo format --check Cargo.toml article-collector.toml release-plz.toml
taplo lint Cargo.toml article-collector.toml release-plz.toml
shellcheck -x scripts/*.sh
git diff --check origin/main...HEAD
git status --short
```

Expected: all pass and the worktree is clean.

- [ ] **Step 2: Push and open the PR**

Push with the ambient invalid token removed:

```bash
env -u GH_TOKEN git push -u origin fix/84-registry-refresh
```

Open a non-draft PR targeting `main`, referencing `Refs #84` rather than closing the issue. The body checklist must include:

```markdown
- [ ] workflow security and registry verification pass
- [ ] cargo fmt, clippy, test, and release build pass
- [ ] refresh authenticates only published stable assets
- [ ] refresh preserves all non-metadata registry payload
- [ ] signing secret remains protected and absent from PR workflows/logs
- [ ] production manual refresh increments revision and extends expiry
```

- [ ] **Step 3: Verify the checklist before checking boxes**

Run:

```bash
task verify-pr PR_NUMBER=<remediation-pr-number>
```

Expected: no FAIL. Manually validate each SKIP from the public diff and recorded test evidence. Only then update verified checkboxes; keep the production manual refresh item unchecked.

- [ ] **Step 4: Pass CI and resolve reviews**

Wait for:

- Rust Check;
- Plugin Registry;
- Workflow Security;
- TOML Check;
- Script Lint;
- Verify PR Checklist.

Run a fresh unresolved-thread query after the latest head is green. Fix every actionable review finding with TDD, rerun scoped review, push, and repeat the gate. Do not merge with unresolved non-outdated review threads.

- [ ] **Step 5: Merge with a head-SHA guard**

Re-read the PR head SHA, mergeability, checks, approvals, and unresolved threads immediately before merging. Merge only the reviewed head and record the merge SHA.

---

### Task 3: Release and prove the independent production refresh

**Files:**
- No source edits expected.
- External state: release-plz PR, core release workflow, `Plugin Registry Refresh` workflow, `plugin-registry-v1` assets, issue #84.

**Interfaces:**
- Consumes: merged refresh workflow on `main`, existing production trust root, registry release revision 2 or later.
- Produces: a successful manual refresh whose post-state is independently verified and documented.

- [ ] **Step 1: Complete the release-plz cycle**

Locate the Release PR created after the remediation merge. Confirm its diff contains only the expected `Cargo.toml` and `Cargo.lock` version changes.

Run:

```bash
task verify-pr PR_NUMBER=<release-pr-number>
env -u GH_TOKEN gh pr checks <release-pr-number> --watch
```

Manually validate SKIPs before changing boxes. Merge the exact reviewed head. Monitor the Release workflow until the GitHub Release, five target builds, asset uploads, and release registry publisher all succeed. Record the tag, release PR merge SHA, workflow run ID, and registry revision.

- [ ] **Step 2: Capture the authenticated pre-refresh state**

Create a fresh temporary directory with `mktemp -d`. Download `plugins-v1.json` and `plugins-v1.sig` from `plugin-registry-v1`. Run:

```bash
cargo run --locked --example plugin_registry -- \
  verify \
  <pre-dir>/plugins-v1.json \
  <pre-dir>/plugins-v1.sig \
  registry/trusted-keys.json
```

Record:

- exact SHA-256 of both assets;
- revision;
- generated and expiry timestamps;
- `del(.revision, .generated_at, .expires_at)` payload hash.

Do not print or retrieve the signing secret.

- [ ] **Step 3: Manually dispatch the independent refresh**

Run:

```bash
env -u GH_TOKEN gh workflow run plugin-registry-refresh.yml --ref main
```

Resolve the new run by workflow name, event `workflow_dispatch`, branch `main`, and creation time after the command. Watch it to terminal success. Confirm the protected refresh job ran rather than skipped.

- [ ] **Step 4: Verify the post-refresh assets independently**

Download both stable assets into a second fresh temporary directory and run exact-byte verification again.

Assert:

```bash
post_revision == pre_revision + 1
post_generated_at > pre_generated_at
post_expires_at > post_generated_at
post_expires_at > now
```

Assert the pre/post payloads after deleting `revision`, `generated_at`, and `expires_at` are semantically identical. Confirm `plugins=[]` until issue #85 publishes the X connector. Confirm both asset SHA-256 values changed and the new signature verifies.

- [ ] **Step 5: Run the released-binary registry smoke**

Download the latest Linux amd64 core release binary into a fresh directory. In ephemeral `ubuntu:24.04` / `linux/amd64`, run:

```bash
article-collector-linux-amd64 plugin install x
```

Expected:

- non-zero exit;
- output contains `Unknown plugin 'x'`;
- output does not contain trust-not-provisioned, registry download, expiry, or invalid-signature errors.

- [ ] **Step 6: Finalize PR and issue evidence**

Before checking the production item in the remediation PR, rerun:

```bash
task verify-pr PR_NUMBER=<remediation-pr-number>
```

Record all automatic PASS and manual SKIP evidence, then update the final checkbox. Confirm the post-edit Verify PR Checklist workflow succeeds.

Add an issue #84 comment containing:

- remediation PR/head/merge SHA;
- release PR/head/merge SHA and core release tag;
- manual refresh workflow run URL;
- pre/post revision and expiry;
- payload-preservation result;
- exact-byte signature verification result;
- released-binary Docker smoke result;
- explicit secret non-exposure statement.

Keep issue #84 open for the final independent branch review.

---

### Task 4: Final review, close issue #84, and clean up exact branches/worktree

**Files:**
- Inspect: `origin/main...fix/84-registry-refresh`, task reports, PR/release/refresh evidence.
- External state: GitHub issue #84, local/remote branches, linked worktree.

**Interfaces:**
- Consumes: Tasks 1-3 with clean task reviews and production evidence.
- Produces: final approved branch, closed issue #84, and removal of only the merged #84 branches/worktree.

- [ ] **Step 1: Run final whole-branch review**

Generate a review package from the recorded merge base to the latest remediation head. Use an independent reviewer to inspect correctness, security, workflow semantics, tests, documentation, and production evidence. If findings exist, run one fix wave and a scoped re-review before proceeding.

- [ ] **Step 2: Verify completion immediately before closure**

Confirm:

- remediation PR merged and CI green;
- release and manual refresh workflows succeeded;
- stable registry is fresh and exact-byte verified;
- payload is unchanged outside metadata;
- issue #84 contains complete evidence and is still open;
- both `feat/84-registry-provision` and `fix/84-registry-refresh` heads are ancestors of `origin/main`;
- worktree is clean.

- [ ] **Step 3: Close issue #84**

Close GitHub issue #84 only after the final review is approved. Do not update Plane.

- [ ] **Step 4: Remove exact merged work**

From `/Users/ktome1995/Program/article-collector`, remove only the linked worktree:

```text
/Users/ktome1995/orca/workspaces/article-collector/feat-xapi-mcp-connection
```

Then delete only the merged local branches:

```text
feat/84-registry-provision
fix/84-registry-refresh
```

Delete the matching remote branches only if they still exist and both merge-ancestry checks passed. Do not remove unrelated branches or worktrees. Worktree removal is the last filesystem action.

