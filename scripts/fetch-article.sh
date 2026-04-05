#!/usr/bin/env bash
# shellcheck shell=bash
set -euo pipefail

# fetch-article.sh — Route URL to appropriate public API and fetch content
# Usage: fetch-article.sh <URL>
# Output: /tmp/collect/raw.json
#
# Routing strategy:
#   HackerNews → HN Firebase public API (curl)
#   Dev.to     → Dev.to public API (curl)
#   YouTube    → youtube-transcript-api (Python)
#   X/Twitter  → syndication API (public tweets only)
#   Other      → curl + Python text extraction (fallback)

URL="${1:?Usage: fetch-article.sh <URL>}"

# Validate URL format
if [[ ! "$URL" =~ ^https?:// ]]; then
  echo "ERROR: Invalid URL format: $URL" >&2
  exit 1
fi

OUTDIR="/tmp/collect"
mkdir -p "$OUTDIR"

fetch_twitter() {
  local url="$1"
  local tweet_id
  tweet_id=$(echo "$url" | grep -oP '/status/\K\d+')
  echo "Routing: $url → X/Twitter syndication API (tweet_id=$tweet_id)"

  local syndication_url="https://cdn.syndication.twimg.com/tweet-result?id=${tweet_id}&token=0"
  local response
  response=$(curl -sf "$syndication_url" 2>/dev/null || true)

  if [ -n "$response" ] && echo "$response" | jq -e '.text' >/dev/null 2>&1; then
    echo "$response" | python3 -c "
import sys, json
d = json.load(sys.stdin)
out = [{
  'title': d.get('text', '')[:80],
  'url': '${url}',
  'content': d.get('text', ''),
  'author': d.get('user', {}).get('name', ''),
  'type': 'x'
}]
json.dump(out, sys.stdout, ensure_ascii=False, indent=2)
" > "$OUTDIR/raw.json"
  else
    echo "  WARN: Could not fetch tweet content. Saving URL reference only." >&2
    echo "[{\"url\":\"${url}\",\"tweet_id\":\"${tweet_id}\",\"content\":\"(public content unavailable)\",\"type\":\"x\"}]" > "$OUTDIR/raw.json"
  fi
}

fetch_youtube() {
  local url="$1"
  # Extract video ID
  local vid
  vid=$(echo "$url" | grep -oP '(?:v=|youtu\.be/)\K[^&]+')
  if [ -z "$vid" ]; then
    echo "ERROR: Could not extract YouTube video ID from $url" >&2
    exit 1
  fi
  echo "Routing: $url → YouTube oEmbed + youtube-transcript-api (vid=$vid)"
  python3 -c "
import sys, json, urllib.request

vid = '${vid}'
url = '${url}'

# Metadata via oEmbed
title, author = 'untitled', ''
try:
    oembed = json.loads(urllib.request.urlopen(
        urllib.request.Request(
            f'https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v={vid}&format=json',
            headers={'User-Agent': 'Mozilla/5.0'}
        ), timeout=10).read())
    title = oembed.get('title', 'untitled')
    author = oembed.get('author_name', '')
except Exception:
    pass

# Transcript via youtube-transcript-api
text = '(transcript unavailable)'
try:
    from youtube_transcript_api import YouTubeTranscriptApi
    api = YouTubeTranscriptApi()
    t = api.fetch(vid, languages=['en', 'ja'])
    text = ' '.join(s.text for s in t.snippets)
except Exception as e:
    text = f'(transcript unavailable: {e})'

json.dump([{
    'title': title,
    'author': author,
    'url': url,
    'content': text,
    'type': 'youtube'
}], sys.stdout, ensure_ascii=False, indent=2)
" > "$OUTDIR/raw.json"
}

fetch_hackernews() {
  local url="$1"
  local id
  id=$(echo "$url" | grep -oP 'id=\K\d+')
  if [ -z "$id" ]; then
    echo "ERROR: Could not extract HN item ID from $url" >&2
    exit 1
  fi
  echo "Routing: $url → HN Firebase API (id=$id)"
  local response
  response=$(curl -sf "https://hacker-news.firebaseio.com/v0/item/${id}.json")
  echo "$response" | python3 -c "
import sys, json
d = json.load(sys.stdin)
out = [{
  'title': d.get('title', ''),
  'url': d.get('url', f\"https://news.ycombinator.com/item?id={d.get('id','')}\"),
  'author': d.get('by', ''),
  'score': d.get('score', 0),
  'text': d.get('text', ''),
  'time': d.get('time', 0),
  'type': d.get('type', ''),
  'descendants': d.get('descendants', 0)
}]
json.dump(out, sys.stdout, ensure_ascii=False, indent=2)
" > "$OUTDIR/raw.json"
}

fetch_devto() {
  local url="$1"
  # Extract slug: dev.to/author/title-slug -> author/title-slug
  local slug
  slug=$(echo "$url" | sed -E 's|https?://dev\.to/||; s|/$||')
  echo "Routing: $url → Dev.to API (slug=$slug)"
  local response
  response=$(curl -sf "https://dev.to/api/articles/${slug}")
  echo "$response" | python3 -c "
import sys, json
d = json.load(sys.stdin)
out = [{
  'title': d.get('title', ''),
  'url': d.get('url', ''),
  'content': d.get('body_markdown', d.get('body_html', '')),
  'author': d.get('user', {}).get('name', ''),
  'tags': d.get('tag_list', []),
  'published_at': d.get('readable_publish_date', ''),
  'reactions': d.get('public_reactions_count', 0)
}]
json.dump(out, sys.stdout, ensure_ascii=False, indent=2)
" > "$OUTDIR/raw.json"
}

fetch_generic() {
  local url="$1"
  local outfile="${2:-$OUTDIR/raw.json}"
  echo "Routing: $url → generic web fetch (curl + Python)"
  curl -sf -L --max-time 30 "$url" | python3 -c "
import sys, json, html, re
text = sys.stdin.read()
title_m = re.search(r'<title>(.*?)</title>', text, re.I|re.S)
title = html.unescape(title_m.group(1).strip()) if title_m else 'untitled'
# Remove scripts/styles, then tags
body = re.sub(r'<(script|style)[^>]*>.*?</\1>', '', text, flags=re.I|re.S)
body = re.sub(r'<[^>]+>', ' ', body)
body = re.sub(r'\s+', ' ', body).strip()
json.dump([{'title': title, 'content': body[:50000], 'url': '$url'}], sys.stdout, ensure_ascii=False, indent=2)
" > "$outfile"
}

# Dispatch a single URL to the appropriate fetcher, output to specified file
fetch_single() {
  local url="$1"
  local outfile="${2:-$OUTDIR/raw.json}"
  case "$url" in
    *x.com/*/status/* | *twitter.com/*/status/*)
      fetch_twitter "$url" ;;
    *youtube.com/watch* | *youtu.be/*)
      fetch_youtube "$url" ;;
    *news.ycombinator.com/item*)
      fetch_hackernews "$url" ;;
    *dev.to/*)
      fetch_devto "$url" ;;
    *)
      fetch_generic "$url" "$outfile" ;;
  esac
}

# Main dispatch
case "$URL" in
  *x.com/*/status/* | *twitter.com/*/status/*)
    fetch_twitter "$URL" ;;
  *youtube.com/watch* | *youtu.be/*)
    fetch_youtube "$URL" ;;
  *news.ycombinator.com/item*)
    fetch_hackernews "$URL" ;;
  *dev.to/*)
    fetch_devto "$URL" ;;
  *)
    fetch_generic "$URL" ;;
esac

echo "Fetch complete: $OUTDIR/"
ls -la "$OUTDIR/"
