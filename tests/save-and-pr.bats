#!/usr/bin/env bats
# Unit tests for save-and-pr.sh
# Run: bats tests/save-and-pr.bats

SCRIPTS_DIR="scripts"

setup() {
  export OUTDIR=$(mktemp -d)
  mkdir -p /tmp/collect
}

teardown() {
  rm -rf "$OUTDIR" /tmp/collect
}

# ── URL validation ──

@test "rejects invalid URL" {
  export TARGET_REPO="owner/repo"
  echo '[{"title":"test"}]' > /tmp/collect/raw.json
  echo "content" > /tmp/collect/translated.md
  run bash "$SCRIPTS_DIR/save-and-pr.sh" "not-a-url"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Invalid URL"* ]]
}

@test "rejects empty argument" {
  run bash "$SCRIPTS_DIR/save-and-pr.sh"
  [ "$status" -ne 0 ]
}

# ── type detection ──
# Extract determine_type from the script and test directly

_determine_type() {
  local url="$1"
  case "$url" in
    *x.com/* | *twitter.com/*) echo "x" ;;
    *youtube.com/* | *youtu.be/*) echo "youtube" ;;
    *arxiv.org/* | *doi.org/* | *openreview.net/*) echo "paper" ;;
    *) echo "web" ;;
  esac
}

@test "type: x.com → x" {
  result=$(_determine_type "https://x.com/user/status/123")
  [ "$result" = "x" ]
}

@test "type: twitter.com → x" {
  result=$(_determine_type "https://twitter.com/user/status/123")
  [ "$result" = "x" ]
}

@test "type: youtube.com → youtube" {
  result=$(_determine_type "https://www.youtube.com/watch?v=abc")
  [ "$result" = "youtube" ]
}

@test "type: youtu.be → youtube" {
  result=$(_determine_type "https://youtu.be/abc123")
  [ "$result" = "youtube" ]
}

@test "type: arxiv.org → paper" {
  result=$(_determine_type "https://arxiv.org/abs/2301.12345")
  [ "$result" = "paper" ]
}

@test "type: doi.org → paper" {
  result=$(_determine_type "https://doi.org/10.1234/example")
  [ "$result" = "paper" ]
}

@test "type: openreview.net → paper" {
  result=$(_determine_type "https://openreview.net/forum?id=abc")
  [ "$result" = "paper" ]
}

@test "type: generic → web" {
  result=$(_determine_type "https://example.com/article")
  [ "$result" = "web" ]
}

# ── title sanitization ──
# Replicate the sanitization logic from save-and-pr.sh

_sanitize_title() {
  echo "$1" | tr -d '\n\r' | sed 's/"/\\"/g' | head -c 200
}

@test "sanitize: strips newlines" {
  result=$(_sanitize_title "$(printf 'Line One\nLine Two')")
  [ "$result" = "Line OneLine Two" ]
}

@test "sanitize: escapes double quotes" {
  result=$(_sanitize_title 'Title "with" quotes')
  [ "$result" = 'Title \"with\" quotes' ]
}

@test "sanitize: strips carriage returns" {
  result=$(_sanitize_title "$(printf 'Title\r\nWith CR')")
  [ "$result" = "TitleWith CR" ]
}

@test "sanitize: truncates to 200 chars" {
  long=$(printf '%0.s-' {1..300})
  result=$(_sanitize_title "$long")
  [ "${#result}" -eq 200 ]
}

@test "sanitize: handles mixed special chars" {
  result=$(_sanitize_title "$(printf 'He said \"hello\"\nand left\r')")
  [ "$result" = 'He said \"hello\"and left' ]
}
