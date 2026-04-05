#!/usr/bin/env bash
# shellcheck shell=bash
set -euo pipefail

# save-and-pr.sh — Generate frontmatter, save to target repo, create PR
# Usage: save-and-pr.sh <url>
# Env: TARGET_REPO, GITHUB_TOKEN (required)
#      TARGET_DIR (default: /tmp/target-repo)
#      SAVE_PATH_TEMPLATE (default: articles/${TYPE}/)
#      AUTO_MERGE (default: true)

URL="${1:?Usage: save-and-pr.sh <URL>}"
OUTDIR="/tmp/collect"
TARGET_REPO="${TARGET_REPO:?TARGET_REPO env var required}"
TARGET_DIR="${TARGET_DIR:-/tmp/target-repo}"
SAVE_PATH_TEMPLATE="${SAVE_PATH_TEMPLATE:-articles/\${TYPE}/}"
AUTO_MERGE="${AUTO_MERGE:-true}"
NOW=$(date +%Y-%m-%d)
BRANCH="collect/$(date +%Y-%m-%d-%H%M%S)"

# Determine type from URL
determine_type() {
  local url="$1"
  case "$url" in
    *x.com/* | *twitter.com/*) echo "x" ;;
    *youtube.com/* | *youtu.be/*) echo "youtube" ;;
    *arxiv.org/* | *doi.org/* | *openreview.net/*) echo "paper" ;;
    *) echo "web" ;;
  esac
}

TYPE=$(determine_type "$URL")

# Extract title from raw.json
TITLE=$(jq -r 'if type == "array" then .[0].title // .[0].text[0:80] else .title // .text[0:80] // "untitled" end' "$OUTDIR/raw.json" | head -1)

# Sanitize for YAML and git safety
TITLE=$(echo "$TITLE" | tr -d '\n\r' | sed 's/"/\\"/g' | head -c 200)

# Validate URL
if [[ ! "$URL" =~ ^https?:// ]]; then
  echo "ERROR: Invalid URL: $URL" >&2
  exit 1
fi

# Normalize title to slug
SLUG=$(echo "$TITLE" | sed 's/[^a-zA-Z0-9]/-/g' | sed 's/--*/-/g' | sed 's/^-//' | sed 's/-$//' | head -c 60 | tr '[:upper:]' '[:lower:]')
FILENAME="${NOW}_${SLUG}.md"

# Build destination path from template
SAVE_PATH="${SAVE_PATH_TEMPLATE//\$\{TYPE\}/$TYPE}"
DEST_DIR="${TARGET_DIR}/${SAVE_PATH}"

# Clone or update target repo
if [ -d "$TARGET_DIR/.git" ]; then
  cd "$TARGET_DIR"
  git checkout main
  git pull origin main
else
  gh repo clone "$TARGET_REPO" "$TARGET_DIR"
  cd "$TARGET_DIR"
fi

# Create branch
git checkout -b "$BRANCH"

# Create output file with frontmatter
mkdir -p "$DEST_DIR"
cat > "${DEST_DIR}/${FILENAME}" <<FRONTMATTER
---
title: "${TITLE}"
type: ${TYPE}
url: "${URL}"
created: ${NOW}
tags: []
---

FRONTMATTER

cat "$OUTDIR/translated.md" >> "${DEST_DIR}/${FILENAME}"

# Append embedded translated articles
for f in "$OUTDIR"/embedded_*_translated.md; do
  [ -f "$f" ] || continue
  {
    echo ""
    echo "---"
    echo ""
    echo "## 関連記事"
    echo ""
    cat "$f"
  } >> "${DEST_DIR}/${FILENAME}"
done

# Validate translated content is not empty or null
if [ ! -s "${DEST_DIR}/${FILENAME}" ] || grep -qx 'null' "$OUTDIR/translated.md"; then
  echo "ERROR: Translation result is empty or null, aborting" >&2
  exit 1
fi

# Commit + PR
git add "${DEST_DIR}/${FILENAME}"
git commit -m "collect: ${TITLE}"
git push -u origin "$BRANCH"

gh pr create \
  --title "collect: ${NOW} ${TITLE}" \
  --body "$(cat <<EOF
## Collected Article

- \`${SAVE_PATH}${FILENAME}\` — ${TITLE}

Source: ${URL}
EOF
)"

if [ "$AUTO_MERGE" = "true" ]; then
  gh pr merge --merge
fi

# Return to main
git checkout main
git pull origin main

echo "Done: ${DEST_DIR}/${FILENAME}"
