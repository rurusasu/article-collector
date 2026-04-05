#!/usr/bin/env bats
# Unit tests for fetch-article.sh
# Run: bats tests/fetch-article.bats

SCRIPTS_DIR="scripts"

setup() {
  export OUTDIR=$(mktemp -d)
  mkdir -p /tmp/collect
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

# ── URL validation ──

@test "rejects URL without https://" {
  run bash "$SCRIPTS_DIR/fetch-article.sh" "not-a-url"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Invalid URL"* ]]
}

@test "rejects ftp:// URL" {
  run bash "$SCRIPTS_DIR/fetch-article.sh" "ftp://example.com/file"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Invalid URL"* ]]
}

@test "rejects empty argument" {
  run bash "$SCRIPTS_DIR/fetch-article.sh"
  [ "$status" -ne 0 ]
}

# ── URL routing ──

@test "routes HN URL to fetch_hackernews" {
  mock_command curl 'echo "{\"title\":\"Test\",\"by\":\"a\",\"score\":1,\"id\":123,\"text\":\"\",\"time\":0,\"type\":\"story\",\"descendants\":0}"'
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"Test"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://news.ycombinator.com/item?id=123"
  [ "$status" -eq 0 ]
  [[ "$output" == *"HN Firebase API"* ]]
}

@test "routes dev.to URL to fetch_devto" {
  mock_command curl 'echo "{\"title\":\"T\",\"url\":\"\",\"body_markdown\":\"\",\"user\":{\"name\":\"\"},\"tag_list\":[],\"readable_publish_date\":\"\",\"public_reactions_count\":0}"'
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"T"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://dev.to/author/slug"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Dev.to API"* ]]
}

@test "routes youtube.com URL to fetch_youtube" {
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"YT","type":"youtube"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://www.youtube.com/watch?v=abc123"
  [ "$status" -eq 0 ]
  [[ "$output" == *"YouTube"* ]]
}

@test "routes x.com URL to fetch_twitter" {
  mock_command curl 'echo ""'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://x.com/user/status/123456"
  [ "$status" -eq 0 ]
  [[ "$output" == *"syndication"* ]] || [[ "$output" == *"X/Twitter"* ]]
}

@test "routes unknown URL to fetch_generic" {
  mock_command curl 'echo "<html><title>Page</title><body>Hello</body></html>"'
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"Page"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://example.com/article"
  [ "$status" -eq 0 ]
  [[ "$output" == *"generic web fetch"* ]]
}

# ── ID / slug extraction ──

@test "extracts HN item ID from URL" {
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"t"}]
EOF'
  mock_command curl '
    for arg in "$@"; do
      if [[ "$arg" == *"firebaseio"* ]]; then
        echo "$arg" > /tmp/collect/curl_url.txt
      fi
    done
    echo "{\"title\":\"t\",\"id\":42575,\"by\":\"u\",\"score\":1,\"text\":\"\",\"time\":0,\"type\":\"story\",\"descendants\":0}"
  '
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://news.ycombinator.com/item?id=42575"
  [ "$status" -eq 0 ]
  [[ "$(cat /tmp/collect/curl_url.txt)" == *"/42575.json"* ]]
}

@test "extracts dev.to slug from URL" {
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"t"}]
EOF'
  mock_command curl '
    for arg in "$@"; do
      if [[ "$arg" == *"dev.to/api"* ]]; then
        echo "$arg" > /tmp/collect/curl_url.txt
      fi
    done
    echo "{\"title\":\"t\",\"url\":\"\",\"body_markdown\":\"\",\"user\":{\"name\":\"\"},\"tag_list\":[],\"readable_publish_date\":\"\",\"public_reactions_count\":0}"
  '
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://dev.to/authorname/my-article-slug"
  [ "$status" -eq 0 ]
  [[ "$(cat /tmp/collect/curl_url.txt)" == *"/authorname/my-article-slug"* ]]
}

@test "extracts YouTube video ID from watch URL" {
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"t","type":"youtube"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
  [ "$status" -eq 0 ]
  [[ "$output" == *"dQw4w9WgXcQ"* ]]
}

@test "extracts YouTube video ID from youtu.be URL" {
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"t","type":"youtube"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://youtu.be/abc123XYZ"
  [ "$status" -eq 0 ]
  [[ "$output" == *"abc123XYZ"* ]]
}

@test "extracts YouTube video ID with extra params" {
  mock_command python3 'cat > /tmp/collect/raw.json <<EOF
[{"title":"t","type":"youtube"}]
EOF'
  run bash "$SCRIPTS_DIR/fetch-article.sh" "https://www.youtube.com/watch?v=xyz789&t=120"
  [ "$status" -eq 0 ]
  [[ "$output" == *"xyz789"* ]]
}
