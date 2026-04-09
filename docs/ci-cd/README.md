# CI/CD Pipeline

## Overview

GitHub Actions で Rust の CI/CD パイプラインを構築している。
バージョン管理は [release-please](https://github.com/googleapis/release-please) で自動化。

## 全体フロー

```mermaid
graph TB
  commit["commit"] --> hook["commit-msg hook<br/>Conventional Commits 検証"]
  hook --> push["push / PR"]
  push --> ci["CI<br/>fmt, clippy, test, build"]
  push --> shell["Shell Lint & Test"]
  ci --> merge["main にマージ"]
  merge --> rp["release-please<br/>Release PR 自動作成<br/>CHANGELOG + Cargo.toml 更新"]
  rp --> rp_merge["Release PR マージ"]
  rp_merge --> tag["v* tag + GitHub Release 作成"]
  tag --> build["cross-compile build<br/>linux-amd64 / arm64"]
  build --> upload["バイナリを Release にアップロード"]
```

## CI (`ci.yml`)

**トリガー:** `main` への push / PR 作成時

```mermaid
graph LR
  trigger["push / PR"] --> rust["Rust Check"]
  trigger --> shell["Shell Lint & Test"]

  subgraph rust_steps["Rust Check"]
    direction TB
    fmt["cargo fmt --check"] --> clippy["cargo clippy --all-targets"]
    clippy --> test["cargo test --locked"]
    test --> build["cargo build --release --locked"]
  end

  subgraph shell_steps["Shell Lint & Test"]
    direction TB
    shellcheck["shellcheck scripts/*.sh"] --> bats["bats tests/"]
  end

  rust --> rust_steps
  shell --> shell_steps
```

| Job | 内容 | キャッシュ |
|-----|------|-----------|
| Rust Check | fmt, clippy, test, release build | cargo registry & build |
| Shell Lint & Test | shellcheck, bats | なし |

- 2 ジョブは **並列実行**
- clippy は `--all-targets` で全ターゲットを lint
- test は `--locked` で Cargo.lock 整合性を検証
- release build で **リリース前のビルド検証** を実施

## Release Please (`release-please.yml`)

**トリガー:** `main` への push 時

Conventional Commits を解析し、自動で:
1. バージョン番号を算出 (`feat` → minor, `fix` → patch, `!` → major)
2. `CHANGELOG.md` を生成
3. `Cargo.toml` の `version` を更新
4. Release PR を作成

Release PR がマージされると、同じワークフロー内で:
1. `v*` タグと GitHub Release を自動作成
2. クロスコンパイルビルドを実行
3. バイナリを GitHub Release にアップロード

> **注意:** デフォルト `GITHUB_TOKEN` によるタグ push は別ワークフローをトリガーしないため、
> release-please とビルドを同一ワークフローに統合している。

```mermaid
graph TB
  rp_merge["Release PR マージ"] --> tag["tag + GitHub Release 作成"]
  tag --> amd64["Build<br/>linux-amd64"]
  tag --> arm64["Build<br/>linux-arm64<br/>(cross-compile)"]
  amd64 --> upload["Upload binaries<br/>to GitHub Release"]
  arm64 --> upload
```

| Job | ターゲット | ツールチェイン |
|-----|-----------|---------------|
| Build (amd64) | `x86_64-unknown-linux-gnu` | stable |
| Build (arm64) | `aarch64-unknown-linux-gnu` | stable + `aarch64-linux-gnu-gcc` |

### セキュリティ

- `permissions: contents: write` + `pull-requests: write` をワークフローレベルで付与
  - release-please: Release PR 作成・タグ push に必要
  - build: `gh release upload` に必要

### ビルド戦略

- `fail-fast: false` — 片方のアーキテクチャが失敗しても他方は継続
- `--locked` — Cargo.lock の整合性を保証
- cargo キャッシュあり（ターゲットごとに分離）

## Pre-commit Hook

`.githooks/commit-msg` でコミットメッセージが Conventional Commits 形式か検証する。

```
<type>(<scope>): <description>
```

| Type | バージョン影響 | 用途 |
|------|--------------|------|
| `feat` | minor bump | 新機能 |
| `fix` | patch bump | バグ修正 |
| `feat!` / `fix!` | **major bump** | 破壊的変更 |
| `docs` | なし | ドキュメント |
| `ci` | なし | CI/CD |
| `chore` | なし | メンテナンス |
| `refactor` | なし | リファクタリング |
| `test` | なし | テスト |

### セットアップ

devcontainer では `post-create.sh` が自動設定する。手動の場合:

```bash
git config core.hooksPath .githooks
```

## ワークフローファイル

| ファイル | 用途 |
|---------|------|
| `.github/workflows/ci.yml` | CI (fmt, clippy, test, build, shellcheck, bats) |
| `.github/workflows/release-please.yml` | Release PR 自動作成 + ビルド + GitHub Release |
| `.githooks/commit-msg` | Conventional Commits 検証 |
| `release-please-config.json` | release-please 設定 |
| `.release-please-manifest.json` | 現在のバージョン管理 |
