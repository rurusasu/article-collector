#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GENERATOR="$ROOT/scripts/generate-x-plugin-registry.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE="$TMP_DIR/source.json"
OUTPUT="$TMP_DIR/output.json"
ASSETS="$TMP_DIR/assets"
mkdir -p "$ASSETS"

cat > "$SOURCE" <<'JSON'
{
  "schema_version": 1,
  "revision": 7,
  "generated_at": "2026-08-01T00:00:00Z",
  "expires_at": "2099-09-01T00:00:00Z",
  "plugins": [{
    "name": "example",
    "versions": [{
      "version": "1.0.0",
      "protocol": "^1.0",
      "manifest": {
        "name": "example",
        "version": "1.0.0",
        "protocol": "^1.0",
        "capabilities": ["recommend-source"],
        "sources": [{"name": "example", "aliases": [], "options_schema": {"type": "object"}}],
        "supports_configuration": false,
        "environment": []
      },
      "artifacts": {}
    }]
  }]
}
JSON

asset_names=(
  article-collector-x-linux-amd64
  article-collector-x-linux-arm64
  article-collector-x-windows-amd64.exe
  article-collector-x-macos-amd64
  article-collector-x-macos-arm64
)

create_assets() {
  rm -f "$ASSETS"/*
  local name
  for name in "${asset_names[@]}"; do
    printf 'fixture-%s\n' "$name" > "$ASSETS/$name"
  done
}

create_assets
bash "$GENERATOR" "$SOURCE" "$OUTPUT" v0.6.20 "$ASSETS"

jq -e '.revision == 8' "$OUTPUT" >/dev/null
jq -e '.plugins | map(select(.name == "example")) | length == 1' "$OUTPUT" >/dev/null
jq -e '.plugins | map(select(.name == "x")) | length == 1' "$OUTPUT" >/dev/null
jq -e '.plugins[] | select(.name == "x") | .versions[0].manifest.sources[0].name == "twitter"' "$OUTPUT" >/dev/null
jq -e '.plugins[] | select(.name == "x") | .versions[0].artifacts | length == 5' "$OUTPUT" >/dev/null
jq -e '.plugins[] | select(.name == "x") | .versions[0].artifacts["x86_64-unknown-linux-gnu"].url == "https://github.com/rurusasu/article-collector/releases/download/v0.6.20/article-collector-x-linux-amd64"' "$OUTPUT" >/dev/null
jq -e '.plugins[] | select(.name == "x") | .versions[0].artifacts["x86_64-unknown-linux-gnu"].byte_length > 0' "$OUTPUT" >/dev/null
cargo run --quiet --locked --example plugin_registry -- validate "$OUTPUT" "$ROOT/registry/trusted-keys.json"

rm "$ASSETS/article-collector-x-macos-arm64"
if bash "$GENERATOR" "$SOURCE" "$OUTPUT" v0.6.20 "$ASSETS"; then
  echo "missing target artifact was accepted" >&2
  exit 1
fi

create_assets
: > "$ASSETS/article-collector-x-macos-arm64"
if bash "$GENERATOR" "$SOURCE" "$OUTPUT" v0.6.20 "$ASSETS"; then
  echo "empty target artifact was accepted" >&2
  exit 1
fi

create_assets
jq '.plugins[0].versions[0].manifest.sources[0] = {"name":"twitter","aliases":["x"],"options_schema":{"type":"object"}}' "$SOURCE" > "$TMP_DIR/conflict.json"
if bash "$GENERATOR" "$TMP_DIR/conflict.json" "$OUTPUT" v0.6.20 "$ASSETS"; then
  echo "duplicate source owner was accepted" >&2
  exit 1
fi

jq '.plugins[0].versions[0].manifest.sources[0] = {"name":"other","aliases":["x"],"options_schema":{"type":"object"}}' "$SOURCE" > "$TMP_DIR/alias-conflict.json"
if bash "$GENERATOR" "$TMP_DIR/alias-conflict.json" "$OUTPUT" v0.6.20 "$ASSETS"; then
  echo "duplicate source alias owner was accepted" >&2
  exit 1
fi

echo "X plugin registry generator tests passed"
