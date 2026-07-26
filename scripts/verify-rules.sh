#!/usr/bin/env bash
# verify-rules.sh — チェックリスト項目 → 検証コマンドのマッピング定義
#
# 各ルールは verify_rules 配列に追加する。フォーマット:
#   "パターン(grep -iE):::コマンド:::説明"
# セパレータは ::: (コロン3つ)
# 制約: パターン・コマンド・説明に ::: を含めないこと

# shellcheck disable=SC2034  # Used by verify-pr.sh via source
VERIFY_RULES=(
  "[Mm]arkdown.*レンダリング|Markdown.*render|Markdown.*valid:::npx --yes marked README.md > /dev/null 2>&1:::Validate Markdown rendering"
  "cargo test|ユニットテスト|unit test:::cargo test --locked:::Run Rust unit tests"
  "cargo fmt|フォーマット|formatting:::cargo fmt --check:::Check Rust formatting"
  "taplo|toml.*format|toml.*lint|toml:::taplo format --check Cargo.toml article-collector.toml release-plz.toml && taplo lint Cargo.toml article-collector.toml release-plz.toml:::Check TOML formatting and lint"
  "cargo clippy|clippy|rust.*lint|lint.*rust:::cargo clippy --all-targets -- -D warnings:::Run clippy lints"
  "cargo build|ビルド|build succeeds|build.*成功:::cargo build --release --locked:::Verify release build"
  "GitHub Actions|workflow.*security|release.*automation|hardening:::bash scripts/verify-workflow-security.sh:::Verify GitHub Actions hardening"
  "plugin registry schema|registry schema.*公開鍵|公開鍵検証:::bash scripts/verify-plugin-registry.sh:::Verify plugin registry schema and public trust set"
  "shellcheck:::if command -v shellcheck >/dev/null 2>&1; then shellcheck -x scripts/*.sh; else nix shell nixpkgs#shellcheck -c shellcheck -x scripts/*.sh; fi:::Lint shell scripts"
)
