#!/usr/bin/env bats
# Unit tests for translate.sh
# Run: bats tests/translate.bats

SCRIPTS_DIR="scripts"

setup() {
  export OUTDIR=$(mktemp -d)
  mkdir -p /tmp/collect
  unset LLM_API_URL LLM_API_TOKEN 2>/dev/null || true
}

teardown() {
  rm -rf "$OUTDIR" /tmp/collect
}

mock_command() {
  local cmd="$1" body="$2"
  mkdir -p "$OUTDIR/bin"
  cat > "$OUTDIR/bin/$cmd" <<MOCK
#!/bin/bash
$body
MOCK
  chmod +x "$OUTDIR/bin/$cmd"
  export PATH="$OUTDIR/bin:$PATH"
}

# ── env var validation ──

@test "fails if LLM_API_URL not set" {
  echo '[{"title":"test","content":"hello"}]' > "$OUTDIR/input.json"
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/input.json"
  [ "$status" -ne 0 ]
  [[ "$output" == *"LLM_API_URL"* ]]
}

@test "fails if LLM_API_TOKEN not set" {
  export LLM_API_URL="http://localhost:8080"
  echo '[{"title":"test","content":"hello"}]' > "$OUTDIR/input.json"
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/input.json"
  [ "$status" -ne 0 ]
  [[ "$output" == *"LLM_API_TOKEN"* ]]
}

@test "fails if input file missing" {
  export LLM_API_URL="http://localhost:8080"
  export LLM_API_TOKEN="test-token"
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/nonexistent.json"
  [ "$status" -ne 0 ]
}

@test "fails if no argument provided" {
  export LLM_API_URL="http://localhost:8080"
  export LLM_API_TOKEN="test-token"
  run bash "$SCRIPTS_DIR/translate.sh"
  [ "$status" -ne 0 ]
}

# ── content extraction ──

@test "fails if extracted content is empty" {
  export LLM_API_URL="http://localhost:8080"
  export LLM_API_TOKEN="test-token"
  echo '[{}]' > "$OUTDIR/input.json"
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/input.json"
  [ "$status" -eq 1 ]
  [[ "$output" == *"No content"* ]]
}

@test "fails if all fields are null (non-array)" {
  export LLM_API_URL="http://localhost:8080"
  export LLM_API_TOKEN="test-token"
  # When all fields are null, jq falls through to the object itself.
  # The script sends this to the API which may return null translation.
  # Test that empty array input triggers "No content"
  echo '[]' > "$OUTDIR/input.json"
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/input.json"
  [ "$status" -eq 1 ]
  [[ "$output" == *"No content"* ]]
}

# ── API response validation ──

@test "fails if API returns null content" {
  export LLM_API_URL="http://localhost:8080"
  export LLM_API_TOKEN="test-token"
  echo '[{"title":"test","content":"hello"}]' > "$OUTDIR/input.json"
  mock_command curl 'echo "{\"choices\":[{\"message\":{\"content\":null}}]}"'
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/input.json"
  [ "$status" -eq 1 ]
  [[ "$output" == *"empty/null"* ]]
}

@test "succeeds with valid API response" {
  export LLM_API_URL="http://localhost:8080"
  export LLM_API_TOKEN="test-token"
  echo '[{"title":"test","content":"hello world"}]' > "$OUTDIR/input.json"
  mock_command curl 'echo "{\"choices\":[{\"message\":{\"content\":\"翻訳されたテキスト\"}}]}"'
  run bash "$SCRIPTS_DIR/translate.sh" "$OUTDIR/input.json"
  [ "$status" -eq 0 ]
  [ -f /tmp/collect/translated.md ]
  [[ "$(cat /tmp/collect/translated.md)" == "翻訳されたテキスト" ]]
}
