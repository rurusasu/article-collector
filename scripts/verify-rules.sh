#!/usr/bin/env bash
# verify-rules.sh — チェックリスト項目 → 検証コマンドのマッピング定義
#
# 各ルールは verify_rules 配列に追加する。フォーマット:
#   "パターン(grep -iE):::コマンド:::説明"
# セパレータは ::: (コロン3つ)

VERIFY_RULES=(
  "[Mm]arkdown.*レンダリング|Markdown.*render|Markdown.*valid:::npx --yes marked README.md > /dev/null 2>&1:::Validate Markdown rendering"
  "cargo test|ユニットテスト|unit test:::cargo test --locked:::Run Rust unit tests"
  "cargo fmt|フォーマット|formatting:::cargo fmt --check:::Check Rust formatting"
  "cargo clippy|clippy|lint:::cargo clippy --all-targets -- -D warnings:::Run clippy lints"
  "cargo build|ビルド|build succeeds|build.*成功:::cargo build --release --locked:::Verify release build"
  "shellcheck:::shellcheck scripts/*.sh:::Lint shell scripts"
  "bats|shell.*test:::bats tests/:::Run bats tests"
)
