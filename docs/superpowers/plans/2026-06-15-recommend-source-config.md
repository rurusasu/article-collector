# Recommend Source Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add source-specific TOML config for `recommend all` and enforce TOML formatting/linting in local and GitHub Actions checks.

**Architecture:** Add a focused `config` module for TOML loading and parsing. Keep source resolution in `recommend.rs` by converting config and CLI options into per-source collection plans before calling existing network collectors.

**Tech Stack:** Rust stable, serde, toml crate, Taplo CLI, GitHub Actions.

---

### Task 1: Config Parsing

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`
- Modify: `Cargo.toml`

- [x] Add failing tests for parsing `[recommend]` and `[recommend.source.arxiv]`.
- [x] Add `toml = "0.8"` and `src/config.rs` with strict serde structs.
- [x] Load `article-collector.toml` by default and `--config <PATH>` when provided.
- [x] Re-run `cargo +stable-x86_64-pc-windows-gnu check --tests --locked`.

### Task 2: Recommend Source Plans

**Files:**
- Modify: `src/recommend.rs`

- [x] Add failing tests for source order, `enabled = false`, arXiv query, source limit, and CLI precedence.
- [x] Add `SourcePlan` helpers that merge CLI values with config values.
- [x] Keep `recommend all --query ...` rejected.
- [x] Re-run `cargo +stable-x86_64-pc-windows-gnu check --tests --locked`.

### Task 3: TOML Checks And Docs

**Files:**
- Create: `article-collector.toml`
- Modify: `Taskfile.yml`
- Modify: `.github/workflows/ci.yml`
- Modify: `scripts/verify-rules.sh`
- Modify: `README.md`
- Modify: `docs/ci-cd/README.md`

- [x] Add the #news arXiv query config with `cs.IR` and `cs.SE`.
- [x] Add `task toml-fmt`, `task toml-lint`, and `task toml-check`.
- [x] Add a GitHub Actions `TOML Check` job using Taplo CLI.
- [x] Add a PR checklist verification rule for TOML items.
- [x] Document config shape, precedence, and Taplo commands.
