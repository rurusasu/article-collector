# テストガイド

## Overview

本プロジェクトの検証は Rust の unit/integration test、plugin registry fixture、shell
script、workflow security check に分かれている。外部サービスへアクセスする live
test は通常の test suite から分離する。

## テスト実行

```bash
cargo test --locked
task check
```

registry と workflow を個別に検証する場合:

```bash
bash tests/generate-x-plugin-registry.sh
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
```

## テスト構成

| 種別 | 場所 | フレームワーク |
|------|------|---------------|
| Rust unit test | `src/**/*.rs` 内 `#[cfg(test)]` | Rust 標準 |
| CLI integration test | `tests/cli.rs` | Cargo integration test |
| architecture test | `tests/architecture.rs` | Cargo integration test |
| plugin CLI test | `tests/plugin_cli.rs` | Cargo integration test |
| X connector test | `tests/x_connector.rs` | Cargo integration test + fake HTTP server |
| install test | `tests/install.sh` | POSIX shell |
| registry generator test | `tests/generate-x-plugin-registry.sh` | POSIX shell |
| workflow security | `scripts/verify-workflow-security.sh` | POSIX shell / jq |

### Rust ユニットテスト内訳

| モジュール | テスト対象 |
|-----------|-----------|
| `fetch.rs` | URL バリデーション、ルーティング、ID 抽出、HTML ストリッピング |
| `save.rs` | タイプ判定、タイトルサニタイズ、slug 生成 |
| `translate.rs` | プロバイダー検出、エンドポイント解決、コンテンツ抽出 |
| `recommend.rs` | recommend target 解決、各 source の正規化、RSS/Atom パース |
| `youtube.rs` | JSON3 字幕抽出、XML 字幕抽出 |
| `paths.rs` | 作業ディレクトリと出力ファイルパス |
| `plugins/*` | signed registry、atomic install、MCP runtime、plugin CLI |
| `x_connector/*` | mode/options、OAuth PKCE、token store、X MCP response、stdio protocol |

## リリース前検証

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
git diff --check
```

リリース workflow は core と `article-collector-x` を Linux amd64/arm64、Windows
amd64、macOS amd64/arm64 の 5 target で build し、公開後に signed registry の
artifact checksum を生成する。production registry の署名検証は、公開済み asset
を download した上で `cargo run --locked --example plugin_registry -- verify-published`
を使う。

## テスト記述ルール

### 必須コメント

production module の unit test には、以下の3項目を日本語で `///` doc コメントとして
記述すること。integration test、shell test は、テスト名または直前のコメントで目的を
明確にする。

```rust
/// 検証: 何を検証するのか（テストの目的）
/// 理由: なぜその検証が必要なのか（背景・動機）
/// リスク: この検証を行わなかった場合に何が起きるか（未検証時の影響）
#[test]
fn test_name() {
    // ...
}
```

### テスト設計方針

- **純粋関数をテスト対象にする**: I/O や外部 API に依存するロジックはヘルパー関数に抽出してからテストする
- **境界値を網羅する**: 空入力、長大入力、特殊文字、欠損フィールド等
- **回帰テストを追加する**: バグ修正時は、修正前に失敗するテストケースを必ず追加する
- **テスト名は振る舞いを説明する**: `test_1` ではなく `strip_html_removes_script_tags` のように
