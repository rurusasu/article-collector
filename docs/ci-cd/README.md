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
  publish --> build["5-target build matrix"]
  build --> upload["upload release assets"]
  build -->|all 5 successful| sourceorder{"current tag = latest<br/>published core release"}
  sourceorder -->|true| registry["Publish Plugin Registry<br/>protected plugin-registry environment"]
  sourceorder -->|false| stale["skip stale registry source"]
  timer["Monday 03:17 UTC<br/>or manual dispatch"] --> refresh["Plugin Registry Refresh"]
  refresh --> authenticate["download + verify-published<br/>require one signature"]
  authenticate --> resign["revision/timestamps only<br/>sign + exact-byte verify"]
  resign --> registry
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
Release PR の merge、または未作成tagに対する手動 dispatch を契機に、workflow が `Cargo.toml` の version から
`v<major>.<minor>.<patch>` 形式の tag を解決し、`gh release create` で tag と
GitHub Release を作成する。その後、同じ workflow 内で core binary と
`article-collector-x` plugin binary を各プラットフォーム向けに build して asset
として upload する。現在の core release は repository の latest release を参照する。

既存 tag を指す push または手動 dispatch は同じ release を再作成しない。次の
version bump を含む Release PR の merge、または未作成tagへの手動 dispatchで新しい
release build と plugin registry publish が進む。
5 target の build matrix がすべて成功すると、protected `plugin-registry` environment の publisher が署名済み official registry を公開する。build が失敗または skip された場合は publisher を実行しない。build 成功後も、現在の Cargo version tag が draft/prerelease を除く最新の published core release である場合だけ公開し、古い queued run は成功のまま refresh/sign と upload を skip する。

`release-plz.toml` は `git_only = true` にしているため、Release PR の version 判定は crates.io ではなく git tag で行う。workflow は `release-plz release` を実行せず、crates.io publish 経路を通らない。tag 名は release-plz の single-crate default に合わせ、`v<major>.<minor>.<patch>` 形式を使う。

| Target | Core asset | Plugin asset |
|--------|------------|--------------|
| `x86_64-unknown-linux-gnu` | `article-collector-linux-amd64` | `article-collector-x-linux-amd64` |
| `aarch64-unknown-linux-gnu` | `article-collector-linux-arm64` | `article-collector-x-linux-arm64` |
| `x86_64-pc-windows-msvc` | `article-collector-windows-amd64.exe` | `article-collector-x-windows-amd64.exe` |
| `x86_64-apple-darwin` | `article-collector-macos-amd64` | `article-collector-x-macos-amd64` |
| `aarch64-apple-darwin` | `article-collector-macos-arm64` | `article-collector-x-macos-arm64` |

したがって core release には 10 個の binary asset があり、plugin registry は
plugin 側の 5 asset の URL、byte length、SHA-256 を参照する。

> デフォルト `GITHUB_TOKEN` による Release 作成は別 workflow を起動しないため、GitHub App token での Release 作成と asset build/upload は同一 workflow に置いている。

### セキュリティ

- workflow のデフォルト `GITHUB_TOKEN` は `contents: read` のみ
- GitHub App token の `contents: write` は tag / GitHub Release / asset upload に必要
- GitHub App token の `pull-requests: read` は main commit に対応する Release PR 判定に必要
- GitHub App token の `pull-requests: write` は Release PR 作成・更新に必要
- build は `--locked` で `Cargo.lock` の整合性を検証する
- registry publisher は `needs: build` で 5 target の成功後だけ実行する
- production signing key は deployment branch を `main` に限定した protected
  `plugin-registry` environment の secret からだけ参照する
- publisher は `plugin-registry-publish` concurrency group で直列化し、実行中の
  publish を cancel しない
- concurrency は順序を保証しないため、publisher は厳密な
  `v<major>.<minor>.<patch>` tag を `publishedAt` で比較し、最新 core release
  だけを公開する。対象 release が 0 件、または最大 `publishedAt` が同一の候補が
  複数ある場合は fail-closed とする
- PR CI は public trust set と development fixture だけを検証し、production
  signing key を参照しない

release publisher は 5 target の X plugin artifact と signed
`plugin-registry-v1` を公開し、production `plugin install x` を可能にする。
checked-in `registry/plugins-v1.json` は引き続き PR 用 fixture である。

## Plugin Registry Refresh (`plugin-registry-refresh.yml`)

**トリガー:** 毎週月曜 03:17 UTC、または default branch からの手動実行

core release が 30 日以上ない期間も official registry を fresh に保つ。
workflow は stable `plugin-registry-v1` release の公開済み
`plugins-v1.json` / `plugins-v1.sig` だけを download し、expiry を recovery
目的で許容する `verify-published` で schema、trust key、signature を認証する。
現行 signer は 1 seed から 1 signature だけを生成するため、認証直後に公開済み
envelope が exactly one signature であることを要求する。複数 signature の
key rotation overlap は metadata 変換前に fail-closed とし、stable assets を
置換しない。full multi-key signing は
[#92](https://github.com/rurusasu/article-collector/issues/92) で追跡する。
repository の registry 内容へ fallback せず、Rust utility が revision を
`u64` として checked increment し、単一の生成時刻から正確に 30 日後の失効時刻
を算出する。その他の payload が意味的に同一であることを確認してから exact
bytes を再署名する。

release publisher と同じ protected `plugin-registry` environment および
`plugin-registry-publish` concurrency group を使い、実行中の publish は
cancel しない。stable release がない、必須 asset が 0 件または欠損・不正、
revision が `u64` でないか increment できない、timestamp が overflow する、
signature が不正または複数ある、payload preservation または再署名後の検証が
失敗した場合は upload せず fail-closed とする。stable release 自体は作成しない。

手動 recovery では Actions の `Plugin Registry Refresh` を `main` で dispatch
する。実行前の assets を別途保存し、workflow 成功後は空の作業 directory へ
公開後の assets を download して次を実行する。

```bash
mkdir manual-refresh
gh release download plugin-registry-v1 \
  --pattern plugins-v1.json \
  --pattern plugins-v1.sig \
  --dir manual-refresh
cargo run --locked --example plugin_registry -- \
  verify \
  manual-refresh/plugins-v1.json \
  manual-refresh/plugins-v1.sig \
  registry/trusted-keys.json
```

署名と freshness、refresh 前後の exactly one signature、revision の `+1`、
新しい timestamps、および実行前後の JSON から 3 metadata field を除いた
payload の意味的同一性を確認する。

## ワークフローファイル

| ファイル | 用途 |
|---------|------|
| `.github/workflows/ci.yml` | PR CI |
| `.github/workflows/release.yml` | Release PR 作成 + GitHub Release + asset build/upload + protected registry publish |
| `.github/workflows/plugin-registry-refresh.yml` | 週次/手動の protected registry metadata refresh と再署名 |
| `.github/workflows/pr-checklist.yml` | PR checklist 検証 |
| `release-plz.toml` | release-plz の git-only release 設定 |
