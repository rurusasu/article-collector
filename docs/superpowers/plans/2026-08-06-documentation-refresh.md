# Documentation Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 現行コード、CLI、リリース運用、公開プラグイン registry に合わせて、アプリの現行 Markdown ドキュメントを更新する。

**Architecture:** Rust の実装・`Cargo.toml`・`Taskfile.yml`・GitHub Actions を仕様の情報源とし、利用者向け README、アーキテクチャ、サイト、CI/CD、テスト文書を同期する。日付付きの設計書・実装計画は履歴として保持し、現行仕様と混同しうる X 関連文書には状態を追記する。

**Tech Stack:** Markdown, Rust stable, Cargo, Taskfile, GitHub Actions, signed plugin registry

## Global Constraints

- 現行 core version は `0.6.21`。X plugin の manifest version は `0.6.20` で、core と独立して管理される。
- `article-collector` と `article-collector-x` の 2 binary 構成を記載する。
- X plugin は署名付き production registry から `article-collector plugin install x` で導入できる状態を反映する。
- リリース成果物、registry revision、検証コマンドは `.github/workflows/release.yml` と `scripts/` の実装に合わせる。
- Plane には記載・更新しない（ユーザー明示の例外）。
- `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、`cargo test --locked`、Markdown のリンク・古い記述検査を実行する。

## Tasks

### Task 1: 現行仕様の棚卸し

- [x] Markdown 全体と直接参照されるコード・workflow・registry manifest を確認する。
- [x] 古い plugin install 例、未公開前提、旧ファイル構成、旧テスト説明を特定する。

### Task 2: 現行ドキュメントを更新

- [x] `README.md` の install、plugin、release、current-version 記述を更新する。
- [x] `docs/architectures/` の plugin、file tree、enum、usage 記述を現行構成に合わせる。
- [x] `docs/ci-cd/README.md` に core/plugin artifact と registry publish の実態を反映する。
- [x] `docs/sites/`、`docs/AGENTS.md`、`docs/tests/README.md` の現行導線と検証手順を更新する。
- [x] 現行仕様と誤読される X の過去設計書に archived status を付ける。

### Task 3: 文書品質を検証

- [x] Markdown 内の旧 version、未公開前提、存在しないファイル・コマンドを検索する。
- [x] Markdown の相対リンクとコード例を確認する。
- [x] Rust の format、clippy、test と最終 diff を確認する。

## Verification Commands

```bash
rg -n "#85|first production|development fixture|example plugin|v0\.7\.0|src/sites/|cargo run --quiet --locked --" --glob '*.md' --glob '!docs/superpowers/plans/2026-08-06-documentation-refresh.md'
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
git diff --check
git diff --stat
```
