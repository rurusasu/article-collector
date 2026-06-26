# CI/CD Pipeline

## Overview

GitHub Actions で Rust CLI の検証とリリースを実行する。
Release PR と version bump は [release-plz](https://release-plz.dev/) で自動化し、tag と GitHub Release は workflow 内の `gh release create` で作成する。

## 全体フロー

```mermaid
graph TB
  pr["pull_request"] --> ci["CI<br/>fmt, clippy, test, build, shellcheck"]
  merge["PR merged to main"] --> release["Release workflow"]
  release --> tagcheck["Cargo.toml version<br/>tag existence check"]
  tagcheck --> exists{"current version<br/>tag exists"}
  exists -->|true| rp["release-plz release-pr<br/>Release PR 作成/更新"]
  exists -->|false| requested{"release-plz PR merge<br/>or manual dispatch"}
  requested -->|true| publish["gh release create<br/>tag / GitHub Release"]
  requested -->|false| noop["no release action"]
  publish --> build["cross-platform build"]
  build --> upload["upload release assets"]
```

## CI (`ci.yml`)

**トリガー:** PR 作成・更新時

| Job | 内容 | キャッシュ |
|-----|------|-----------|
| Rust Check | `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test --locked`, `cargo build --release --locked` | cargo registry & build |
| TOML Check | `taplo format --check`, `taplo lint` | なし |
| Script Lint | `scripts/*.sh` の shellcheck | なし |

旧シェル実装と bats テストは削除済み。記事取得・翻訳・保存の検証は Rust ユニットテストで行う。

## Release (`release.yml`)

**トリガー:** `main` への push、または手動実行

通常の feature/fix PR が merge されると `release-plz release-pr` が Release PR を作成または更新する。
Release PR が merge されると workflow が `Cargo.toml` の version から `v0.7.0` 形式の tag を解決し、`gh release create` で tag と GitHub Release を作成する。その後、同じ workflow 内で各プラットフォーム向けバイナリをビルドして asset としてアップロードする。

`release-plz.toml` は `git_only = true` にしているため、Release PR の version 判定は crates.io ではなく git tag で行う。workflow は `release-plz release` を実行せず、crates.io publish 経路を通らない。tag 名は release-plz の single-crate default に合わせ、`v0.7.0` のような形式を使う。

| Asset | Target |
|-------|--------|
| `article-collector-linux-amd64` | `x86_64-unknown-linux-gnu` |
| `article-collector-linux-arm64` | `aarch64-unknown-linux-gnu` |
| `article-collector-windows-amd64.exe` | `x86_64-pc-windows-msvc` |
| `article-collector-macos-amd64` | `x86_64-apple-darwin` |
| `article-collector-macos-arm64` | `aarch64-apple-darwin` |

> デフォルト `GITHUB_TOKEN` による Release 作成は別 workflow を起動しないため、GitHub App token での Release 作成と asset build/upload は同一 workflow に置いている。

### セキュリティ

- workflow のデフォルト `GITHUB_TOKEN` は `contents: read` のみ
- GitHub App token の `contents: write` は tag / GitHub Release / asset upload に必要
- GitHub App token の `pull-requests: read` は main commit に対応する Release PR 判定に必要
- GitHub App token の `pull-requests: write` は Release PR 作成・更新に必要
- build は `--locked` で `Cargo.lock` の整合性を検証する

## ワークフローファイル

| ファイル | 用途 |
|---------|------|
| `.github/workflows/ci.yml` | PR CI |
| `.github/workflows/release.yml` | Release PR 作成 + GitHub Release + asset build/upload |
| `.github/workflows/pr-checklist.yml` | PR checklist 検証 |
| `release-plz.toml` | release-plz の git-only release 設定 |
