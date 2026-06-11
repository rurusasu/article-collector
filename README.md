# article-collector

URL -> 記事取得 -> 翻訳 -> PR 作成を自動化する Rust 製 CLI ツール。

任意の OpenAI 互換 API / Anthropic API / Claude Code で翻訳できる。通常の CLI として使うツールであり、GitHub CLI extension ではない。

## セットアップ

### 推奨: cargo install

GitHub Release がまだ存在しない場合でも、この方法でインストールできる。

```bash
cargo install --git https://github.com/rurusasu/article-collector --locked
article-collector fetch https://example.com/article
```

Windows で `cargo` が未導入の場合は、先に Rust を入れる:

```powershell
winget install Rustlang.Rustup
# 新しい PowerShell を開き直してから実行
cargo install --git https://github.com/rurusasu/article-collector --locked
article-collector fetch https://example.com/article
```

### GitHub Releases から取得

Release workflow により GitHub Release が作成された後は、ビルド済みバイナリも利用できる。
Release がまだない時点ではこの手順は使えない。

| Platform | Asset |
|----------|-------|
| Linux amd64 | `article-collector-linux-amd64` |
| Linux arm64 | `article-collector-linux-arm64` |
| Windows amd64 | `article-collector-windows-amd64.exe` |
| macOS amd64 (Intel) | `article-collector-macos-amd64` |
| macOS arm64 (Apple Silicon) | `article-collector-macos-arm64` |

```bash
# Linux / macOS
gh release download --repo rurusasu/article-collector --pattern "article-collector-linux-amd64"
chmod +x article-collector-linux-amd64
mv article-collector-linux-amd64 ~/.local/bin/article-collector
```

```powershell
# Windows PowerShell
gh release download --repo rurusasu/article-collector --pattern "article-collector-windows-amd64.exe"
New-Item -ItemType Directory -Force "$env:USERPROFILE\bin" | Out-Null
Move-Item article-collector-windows-amd64.exe "$env:USERPROFILE\bin\article-collector.exe"
```

## Quick Start

fetch のみなら翻訳 API や GitHub 認証なしで試せる。

```bash
article-collector fetch https://news.ycombinator.com/item?id=42575537
```

全工程を実行する場合:

```bash
export LLM_API_URL="https://api.openai.com/v1"
export LLM_API_TOKEN="sk-..."
export TARGET_REPO="your-org/your-repo"

article-collector collect https://news.ycombinator.com/item?id=42575537
```

Claude Code CLI を使う場合:

```bash
export LLM_API_URL="claude-code"
article-collector collect https://example.com/article
```

## CLI Usage

```bash
# 全工程 (取得 -> 翻訳 -> 保存 -> PR)
article-collector collect <URL>

# 個別ステップ
article-collector fetch <URL>
article-collector translate [INPUT_JSON]
article-collector save-and-pr <URL>
```

`translate` の `INPUT_JSON` を省略した場合は、作業ディレクトリ内の `raw.json` を読む。

## 作業ディレクトリ

取得結果と翻訳結果は作業ディレクトリに保存される。

| OS | Default |
|----|---------|
| Linux / macOS / Git Bash / WSL | `/tmp/collect` |
| Windows native | `%TEMP%\article-collector` |

任意の場所に変えたい場合:

```bash
export ARTICLE_COLLECTOR_OUTDIR="$HOME/.cache/article-collector"
```

```powershell
$env:ARTICLE_COLLECTOR_OUTDIR = "$env:TEMP\article-collector"
```

出力ファイル:

| File | 内容 |
|------|------|
| `raw.json` | `fetch` の取得結果 |
| `translated.md` | `translate` の翻訳結果 |

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ARTICLE_COLLECTOR_OUTDIR` | No | OS 依存 | `raw.json` / `translated.md` の保存先 |
| `LLM_API_URL` | No | `claude-code` | API エンドポイント。`claude-code` で Claude Code CLI 使用 |
| `LLM_API_TOKEN` | Yes* | - | API 認証トークン |
| `LLM_MODEL` | No | provider 依存 | 翻訳に使うモデル |
| `TRANSLATE_LANG` | No | `ja` | 翻訳先言語コード |
| `TARGET_REPO` | Yes** | - | 保存先 GitHub リポジトリ (`owner/repo`) |
| `TARGET_DIR` | No | OS 依存 | 保存先 repo のローカル clone 先 |
| `SAVE_PATH_TEMPLATE` | No | `articles/${TYPE}/` | 保存先パステンプレート |
| `AUTO_MERGE` | No | `true` | PR 作成後に merge する |

\* `LLM_API_URL=claude-code` の場合は不要。
\*\* `save-and-pr` / `collect` の保存ステップのみ必要。

`save-and-pr` は `gh` CLI を使うため、事前に認証が必要:

```bash
gh auth login
```

## Supported Sites

| Domain | Method | Auth |
|--------|--------|------|
| HackerNews | Firebase public API | None |
| Dev.to | Dev.to public API | None |
| YouTube | oEmbed + caption fetch | None |
| X/Twitter | Syndication API | Public tweets only |
| Other | HTTP fetch + HTML scraping | None |

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Taskfile は Rust CLI の薄いラッパーとして使える:

```bash
task fetch URL=https://example.com/article
task translate
task check
```

## Release

`main` 宛の PR が merge されると `.github/workflows/release.yml` が実行される。
通常の PR merge では release-please が Release PR を作成・更新し、Release PR が merge されると GitHub Release と各 OS 向け asset が作成される。

詳細: [docs/ci-cd/README.md](docs/ci-cd/README.md)

## Troubleshooting

### `cargo` が見つからない

Rust toolchain をインストールしてから、新しい shell を開き直す。

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Windows では `winget install Rustlang.Rustup` 後に PowerShell を開き直す。

### Release download が失敗する

GitHub Release がまだ作成されていない場合、`gh release download ...` は使えない。
その場合は `cargo install --git https://github.com/rurusasu/article-collector --locked` を使う。

### `gh article-collector` として使いたい

現在の構成は通常の CLI (`article-collector`) 用。
GitHub CLI extension として `gh article-collector ...` で使うには、別途 `gh-article-collector` 名の実行ファイルまたは GitHub CLI extension 用の repo 構成が必要。

## License

MIT
