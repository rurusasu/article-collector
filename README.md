# article-collector

URL → 記事取得 → 翻訳 → PR 作成を自動化する CLI ツール。

任意の OpenAI 互換 API で翻訳。go-task で実行。AI エージェントの sandbox 内でもスタンドアロンでも動作。

## Quick Start

```bash
# 依存ツール: curl, jq, python3, git, gh, go-task
pip3 install youtube-transcript-api

# 環境変数を設定
export LLM_API_URL="https://api.openai.com/v1"
export LLM_API_TOKEN="sk-..."
export TARGET_REPO="your-org/your-repo"
export GITHUB_TOKEN="ghp_..."

# 記事を取得→翻訳→PR作成
task collect URL=https://news.ycombinator.com/item?id=42575537
```

## Requirements

| Tool | Version | Install |
|---|---|---|
| bash | 4+ | OS built-in |
| curl | any | OS built-in |
| jq | 1.6+ | `apt install jq` / `brew install jq` |
| python3 | 3.8+ | OS built-in |
| youtube-transcript-api | latest | `pip3 install youtube-transcript-api` |
| git | 2.0+ | OS built-in |
| gh | 2.0+ | `apt install gh` / `brew install gh` |
| go-task | 3.0+ | https://taskfile.dev/installation/ |

## Configuration

| Variable | Required | Default | Description |
|---|---|---|---|
| `LLM_API_URL` | Yes | — | OpenAI 互換 API エンドポイント |
| `LLM_API_TOKEN` | Yes | — | API 認証トークン |
| `LLM_MODEL` | No | `gpt-4o` | 翻訳に使うモデル |
| `TRANSLATE_LANG` | No | `ja` | 翻訳先言語コード |
| `TARGET_REPO` | Yes* | — | 保存先 GitHub リポジトリ (owner/repo) |
| `TARGET_DIR` | No | `/tmp/target-repo` | ローカルクローン先 |
| `SAVE_PATH_TEMPLATE` | No | `articles/${TYPE}/` | 保存先パステンプレート |
| `AUTO_MERGE` | No | `true` | PR 作成後に auto-merge |
| `GITHUB_TOKEN` | Yes* | — | GitHub API 認証 |

\* save-and-pr ステップのみ必要

## Supported Sites

| Domain | Method | Auth |
|---|---|---|
| HackerNews | Firebase public API | None |
| Dev.to | Dev.to public API | None |
| YouTube | youtube-transcript-api | None |
| X/Twitter | Syndication API | Public tweets only |
| Other | curl + Python extraction | None |

## Pipeline Flow

```
URL → fetch-article.sh → /tmp/collect/raw.json
                              ↓
                        translate.sh → /tmp/collect/translated.md
                              ↓
                        save-and-pr.sh → target repo PR
```

## Individual Steps

```bash
# Fetch only
task fetch URL=https://dev.to/author/article-slug
cat /tmp/collect/raw.json

# Translate only (requires fetch first)
task translate

# Save and create PR (requires fetch + translate)
task save-and-pr URL=https://dev.to/author/article-slug
```

## Usage with OpenClaw

### In Sandbox

Clone to a temporary directory and execute:

```bash
git clone --depth 1 https://github.com/rurusasu/article-collector.git /tmp/article-collector
bash /tmp/article-collector/scripts/fetch-article.sh <URL>
bash /tmp/article-collector/scripts/translate.sh /tmp/collect/raw.json
```

Or symlink to workspace for persistent access:

```bash
ln -s /path/to/article-collector /workspace/article-collector
```

### AGENTS.md

Add to your workspace AGENTS.md:

```markdown
### Article Collector
bash /workspace/article-collector/scripts/fetch-article.sh <URL>
bash /workspace/article-collector/scripts/translate.sh /tmp/collect/raw.json
```

### Sandbox Environment Variables

Set in your agent's `sandbox.docker.env`:

```json
{
  "LLM_API_URL": "http://host.docker.internal:41789/api/v1/chat/completions",
  "LLM_API_TOKEN": "${GATEWAY_TOKEN}",
  "LLM_MODEL": "gpt-4o",
  "TRANSLATE_LANG": "ja"
}
```

## Adding New Sites

Edit `scripts/fetch-article.sh` and add a case pattern:

```bash
# In the main dispatch (bottom of file)
case "$URL" in
  *newsite.com/*)
    fetch_newsite "$URL" ;;
  ...
esac
```

Then implement `fetch_newsite()` following the existing pattern:

```bash
fetch_newsite() {
  local url="$1"
  echo "Routing: $url → NewSite API"
  local response
  response=$(curl -sf "https://api.newsite.com/articles/...")
  echo "$response" | python3 -c "
import sys, json
d = json.load(sys.stdin)
out = [{'title': d['title'], 'content': d['body'], 'url': '$url', 'type': 'newsite'}]
json.dump(out, sys.stdout, ensure_ascii=False, indent=2)
" > "$OUTDIR/raw.json"
}
```

## Testing

```bash
task lint    # shellcheck
task test    # bats unit tests
```

## License

MIT
