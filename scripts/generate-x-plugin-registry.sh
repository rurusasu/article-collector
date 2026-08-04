#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <source-index> <output-index> <release-tag> <asset-dir>" >&2
  exit 2
fi

SOURCE_INDEX="$1"
OUTPUT_INDEX="$2"
RELEASE_TAG="$3"
ASSET_DIR="$4"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/../registry/x-plugin-manifest.json"

fail() {
  echo "generate-x-plugin-registry: $*" >&2
  exit 1
}

[ -f "$SOURCE_INDEX" ] || fail "source registry does not exist: $SOURCE_INDEX"
[ -f "$MANIFEST" ] || fail "X plugin manifest does not exist: $MANIFEST"
[ -d "$ASSET_DIR" ] || fail "asset directory does not exist: $ASSET_DIR"
[[ "$RELEASE_TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release tag must be a stable vX.Y.Z tag"
command -v jq >/dev/null || fail "jq is required"

PLUGIN_NAME="$(jq -er '.name' "$MANIFEST")"
PLUGIN_VERSION="$(jq -er '.version' "$MANIFEST")"
SOURCE_NAME="$(jq -er '.sources[0].name' "$MANIFEST")"
ALIASES_JSON="$(jq -c '.sources[0].aliases // []' "$MANIFEST")"
[ "$PLUGIN_NAME" = "x" ] || fail "manifest plugin name must be x"
[ "$SOURCE_NAME" = "twitter" ] || fail "manifest source name must be twitter"
[[ "$PLUGIN_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "manifest version must be stable semver"

if ! jq -e --arg plugin "$PLUGIN_NAME" --arg source "$SOURCE_NAME" --argjson aliases "$ALIASES_JSON" '
  [
    .plugins[]?
    | select(.name != $plugin)
    | .versions[]?.manifest.sources[]?
    | select(
        .name == $source
        or ((.aliases // []) as $existing | any($existing[]?; . as $alias | ($aliases | index($alias)) != null))
      )
  ] | length == 0
' "$SOURCE_INDEX" >/dev/null; then
  fail "source owner twitter/x is already registered by another plugin"
fi

targets=(
  "x86_64-unknown-linux-gnu:article-collector-x-linux-amd64"
  "aarch64-unknown-linux-gnu:article-collector-x-linux-arm64"
  "x86_64-pc-windows-msvc:article-collector-x-windows-amd64.exe"
  "x86_64-apple-darwin:article-collector-x-macos-amd64"
  "aarch64-apple-darwin:article-collector-x-macos-arm64"
)

expected_files=()
artifacts='{}'
for mapping in "${targets[@]}"; do
  target="${mapping%%:*}"
  asset="${mapping#*:}"
  expected_files+=("$asset")
  path="$ASSET_DIR/$asset"
  [ -f "$path" ] || fail "missing artifact for $target: $asset"
  [ -s "$path" ] || fail "artifact is empty for $target: $asset"

  if command -v sha256sum >/dev/null; then
    sha256="$(sha256sum "$path" | awk '{print $1}')"
  else
    sha256="$(shasum -a 256 "$path" | awk '{print $1}')"
  fi
  byte_length="$(wc -c < "$path" | tr -d '[:space:]')"
  url="https://github.com/rurusasu/article-collector/releases/download/$RELEASE_TAG/$asset"
  artifact="$(jq -cn --arg url "$url" --arg sha256 "$sha256" --argjson byte_length "$byte_length" '{url: $url, byte_length: $byte_length, sha256: $sha256}')"
  artifacts="$(jq -c --arg target "$target" --argjson artifact "$artifact" '. + {($target): $artifact}' <<< "$artifacts")"
done

actual_files="$(find "$ASSET_DIR" -maxdepth 1 -type f -exec basename {} \; | sort)"
expected_file_list="$(printf '%s\n' "${expected_files[@]}" | sort)"
[ "$actual_files" = "$expected_file_list" ] || fail "asset directory must contain exactly the five X plugin artifacts"

manifest_json="$(jq -c . "$MANIFEST")"
new_version="$(jq -cn \
  --arg version "$PLUGIN_VERSION" \
  --arg protocol "$(jq -er '.protocol' "$MANIFEST")" \
  --argjson manifest "$manifest_json" \
  --argjson artifacts "$artifacts" \
  '{version: $version, protocol: $protocol, manifest: $manifest, artifacts: $artifacts}')"

output_dir="$(dirname "$OUTPUT_INDEX")"
mkdir -p "$output_dir"
staging="$(mktemp "$output_dir/.plugins-v1.json.XXXXXX")"
trap 'rm -f "$staging"' EXIT
jq \
  --arg plugin "$PLUGIN_NAME" \
  --arg version "$PLUGIN_VERSION" \
  --argjson new_version "$new_version" \
  'if any(.plugins[]?; .name == $plugin) then
     .plugins |= map(
       if .name == $plugin then
         .versions = ((.versions // [] | map(select(.version != $version))) + [$new_version])
       else . end
     )
   else
     .plugins += [{name: $plugin, versions: [$new_version]}]
   end
   | .revision = (.revision + 1)' \
  "$SOURCE_INDEX" > "$staging"
jq -e . "$staging" >/dev/null
mv "$staging" "$OUTPUT_INDEX"
trap - EXIT

echo "Generated X plugin registry entry $PLUGIN_NAME@$PLUGIN_VERSION for $RELEASE_TAG"
