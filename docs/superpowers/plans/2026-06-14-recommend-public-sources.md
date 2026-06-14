# Recommend Public Sources Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Zenn and parameterized arXiv recommendation sources to `recommend all` while leaving YouTube for a later phase.

**Architecture:** Extend the site registry with new recommend source variants, parse RSS/Atom through `quick-xml`, and keep the existing per-source `--limit` behavior. Add a source-specific CLI query option for arXiv custom searches.

**Tech Stack:** Rust stable, clap, reqwest, serde_json, quick-xml, cargo test/clippy/fmt.

---

### Task 1: Lock Expected Registry And CLI Behavior

**Files:**
- Modify: `src/sites.rs`
- Modify: `src/recommend.rs`
- Modify: `src/main.rs`

- [x] Add failing tests that expect `recommendable_site_names()` to include `hackernews`, `devto`, `zenn`, and `arxiv`.
- [x] Add failing tests that `recommend arxiv` resolves to the default arXiv source.
- [x] Add failing tests that custom arXiv queries are accepted through the recommend collection path.
- [x] Run targeted tests and confirm they fail for missing Zenn/arXiv support.

### Task 2: Implement Zenn And arXiv Sources

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/sites.rs`
- Modify: `src/recommend.rs`

- [x] Add `quick-xml`.
- [x] Add `RecommendSource::ZennFeed` and `RecommendSource::ArxivSearch`.
- [x] Add Zenn and arXiv site registry entries.
- [x] Implement XML parsing helpers and source-specific normalizers.
- [x] Run targeted tests and confirm they pass.

### Task 3: Update Docs And Verify

**Files:**
- Modify: `README.md`
- Optional: `docs/tests/README.md`

- [x] Document Zenn and arXiv recommend behavior.
- [x] Document that YouTube is deferred to a later API-key/channel phase.
- [x] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --locked`.
