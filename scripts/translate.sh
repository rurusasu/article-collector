#!/usr/bin/env bash
# shellcheck shell=bash
set -euo pipefail

# translate.sh — Translate fetched content via OpenAI-compatible LLM API
# Usage: translate.sh <input.json>
# Output: /tmp/collect/translated.md
# Env: LLM_API_URL, LLM_API_TOKEN (required), LLM_MODEL (default: gpt-4o), TRANSLATE_LANG (default: ja)

INPUT="${1:?Usage: translate.sh <input.json>}"
OUTDIR="/tmp/collect"
mkdir -p "$OUTDIR"
LLM_API_URL="${LLM_API_URL:?LLM_API_URL env var required}"
LLM_API_TOKEN="${LLM_API_TOKEN:?LLM_API_TOKEN env var required}"
LLM_MODEL="${LLM_MODEL:-gpt-4o}"
TRANSLATE_LANG="${TRANSLATE_LANG:-ja}"

# Auto-detect API endpoint path
if [[ "$LLM_API_URL" =~ /chat/completions$ ]]; then
  API_ENDPOINT="$LLM_API_URL"
elif [[ "$LLM_API_URL" =~ /v1/?$ ]]; then
  API_ENDPOINT="${LLM_API_URL%/}/chat/completions"
else
  API_ENDPOINT="${LLM_API_URL%/}/chat/completions"
fi

CONTENT=$(jq -r 'if type == "array" then map(.text // .content // .title | select(. != null)) | join("\n\n---\n\n") else .text // .content // .title // . end' "$INPUT")

if [ -z "$CONTENT" ] || [ "$CONTENT" = "null" ]; then
  echo "ERROR: No content extracted from $INPUT" >&2
  exit 1
fi

# Translate via LLM API
RESPONSE=$(curl -sf "$API_ENDPOINT" \
  -H "Authorization: Bearer ${LLM_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "$(jq -n --arg content "$CONTENT" --arg model "$LLM_MODEL" --arg lang "$TRANSLATE_LANG" '{
    "model": $model,
    "messages": [{
      "role": "user",
      "content": ("以下の記事を" + $lang + "に翻訳してください。Markdown形式を維持し、技術用語は適切に翻訳してください。\n\n" + $content)
    }]
  }')")

TRANSLATED=$(echo "$RESPONSE" | jq -r '.choices[0].message.content')
if [ -z "$TRANSLATED" ] || [ "$TRANSLATED" = "null" ]; then
  echo "ERROR: Translation API returned empty/null response" >&2
  echo "Response: $(echo "$RESPONSE" | head -c 500)" >&2
  exit 1
fi
echo "$TRANSLATED" > "$OUTDIR/translated.md"

# Translate embedded articles if they exist
for f in "$OUTDIR"/embedded_*.json; do
  [ -f "$f" ] || continue
  BASENAME=$(basename "$f" .json)
  EMB_CONTENT=$(jq -r '.text // .content // .title // ""' "$f")
  if [ -z "$EMB_CONTENT" ] || [ "$EMB_CONTENT" = "null" ]; then
    continue
  fi

  EMB_RESPONSE=$(curl -sf "$API_ENDPOINT" \
    -H "Authorization: Bearer ${LLM_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg content "$EMB_CONTENT" --arg model "$LLM_MODEL" --arg lang "$TRANSLATE_LANG" '{
      "model": $model,
      "messages": [{
        "role": "user",
        "content": ("以下の記事を" + $lang + "に翻訳してください。Markdown形式を維持し、技術用語は適切に翻訳してください。\n\n" + $content)
      }]
    }')")

  echo "$EMB_RESPONSE" | jq -r '.choices[0].message.content' > "$OUTDIR/${BASENAME}_translated.md"
done

echo "Translation complete"
