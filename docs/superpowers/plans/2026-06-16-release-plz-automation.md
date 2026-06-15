# Release-plz Automation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace release-please with release-plz while keeping the existing same-workflow multi-platform release asset build.

**Architecture:** `release-plz` owns git-only versioning, Release PR creation, tag creation, and GitHub Release creation. The existing release matrix build remains in `.github/workflows/release.yml` and runs only when `release-plz release` reports `releases_created == true`, deriving the tag from the action's JSON output.

**Tech Stack:** GitHub Actions, release-plz action, GitHub App token, Rust stable, `gh release upload`, Bash workflow guard scripts.

---

### Task 1: Add Workflow Security RED

**Files:**
- Modify: `scripts/verify-workflow-security.sh`

- [ ] **Step 1: Add release-plz expectations**

Replace release-please-specific checks with checks for a pinned `release-plz/action`, `command: release-pr`, `command: release`, release output parsing, and GitHub App token use.

- [ ] **Step 2: Verify RED**

Run: `bash scripts/verify-workflow-security.sh`

Expected: FAIL because `.github/workflows/release.yml` still uses `googleapis/release-please-action`.

### Task 2: Switch Release Workflow

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Replace release-please job**

Use a `release-plz` job that runs `release-plz/action` with `command: release`, outputs `releases_created`, and extracts the first release tag with `jq`.

- [ ] **Step 2: Add release PR job**

Add a `release-plz-pr` job that runs `release-plz/action` with `command: release-pr`, using the existing GitHub App token and a non-canceling concurrency group.

- [ ] **Step 3: Wire matrix build**

Keep the current build matrix and change its condition to `needs.release-plz.outputs.releases_created == 'true'`; checkout and upload against `needs.release-plz.outputs.tag_name`.

### Task 3: Add Release-plz Config

**Files:**
- Create: `release-plz.toml`
- Delete: `release-please-config.json`
- Delete: `.release-please-manifest.json`

- [ ] **Step 1: Configure git-only release-plz**

Create `release-plz.toml` with `git_only = true`, `release_always = false`, `changelog_update = false`, `semver_check = false`, and `pr_labels = ["release"]`.

- [ ] **Step 2: Keep tag naming default**

Do not set `git_tag_name`; future tags use release-plz's single-crate default `v{{ version }}`.

### Task 4: Update Docs

**Files:**
- Modify: `docs/ci-cd/README.md`
- Modify: `README.md`

- [ ] **Step 1: Document release-plz ownership**

Replace release-please wording with release-plz, git-only versioning, and Release PR flow.

- [ ] **Step 2: Document migration anchor**

Document that switching away from `article-collector-v*` requires a one-time `v0.6.1` baseline tag before relying on git-only version detection.

### Task 5: Verify

**Files:**
- None

- [ ] **Step 1: Verify GREEN**

Run: `bash scripts/verify-workflow-security.sh`

Expected: PASS.

- [ ] **Step 2: Run broader checks**

Run: `taplo format --check Cargo.toml article-collector.toml release-plz.toml`, `taplo lint Cargo.toml article-collector.toml release-plz.toml`, and `git diff --check`.

Expected: all commands pass.
