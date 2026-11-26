# article-collector

URL -> 記事取得 -> 翻訳 -> PR 作成を自動化する Rust 製 CLI ツール。

ACP agent 経由で翻訳できる。

現行 core release は [latest release](https://github.com/rurusasu/article-collector/releases/latest)
で確認できる。X の recommendation plugin は release tag と同じ manifest version で、
同じ release の 5 target plugin artifact から配布される。

## 目次

- [セットアップ](#セットアップ)
- [まず使う](#まず使う)
- [処理フロー](#処理フロー)
- [機能別の使い方](#機能別の使い方)
- [コマンド一覧](#コマンド一覧)
- [設定](#設定)
- [対応 URL と保存分類](#対応-url-と保存分類)
- [詳細ドキュメント](#詳細ドキュメント)
- [開発](#開発)
- [リリース](#リリース)
- [トラブルシューティング](#トラブルシューティング)
- [ライセンス](#ライセンス)

## セットアップ

### 推奨: インストーラーを1コマンドで実行（macOS / Linux）

OS と CPU に合う latest release のビルド済みバイナリを自動選択するため、Rust toolchain は不要。

```bash
curl -fsSL https://raw.githubusercontent.com/rurusasu/article-collector/main/install.sh | sh
```

`~/.local/bin/article-collector` にインストールされる。`article-collector` が見つからない場合は PATH に追加する:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Windows PowerShell

Windows では対応する release asset をダウンロードする:

```powershell
$asset = "article-collector-windows-amd64.exe"
$binDir = "$env:USERPROFILE\bin"
New-Item -ItemType Directory -Force $binDir | Out-Null
Invoke-WebRequest `
  -Uri "https://github.com/rurusasu/article-collector/releases/latest/download/$asset" `
  -OutFile "$binDir\article-collector.exe"
$env:Path = "$binDir;$env:Path"
& "$binDir\article-collector.exe" --version
```

`$env:Path` の変更は現在の PowerShell セッションだけに適用される。次回以降も
PATH に含める場合は、`$binDir` をユーザー環境変数 `Path` に追加する。

`article collect --through pr` を実行する場合は [GitHub CLI](https://cli.github.com/) を別途
インストールし、選択した ACP agent（例: Codex）も利用可能にする。どちらも
`article-collector` のインストーラーには含まれない。

### 代替: GitHub Releases の手動取得

installer を使わず、対応する asset を手動で取得する場合:

| Platform | Asset |
|----------|-------|
| Linux amd64 | `article-collector-linux-amd64` |
| Linux arm64 | `article-collector-linux-arm64` |
| Windows amd64 | `article-collector-windows-amd64.exe` |
| macOS amd64 (Intel) | `article-collector-macos-amd64` |
| macOS arm64 (Apple Silicon) | `article-collector-macos-arm64` |

### 代替: cargo install

Rust toolchain / Cargo が入っている環境では、source から直接インストールできる。
Release asset が使えない環境での fallback としても使える。

```bash
cargo install --git https://github.com/rurusasu/article-collector --locked
```

Windows で `cargo` が未導入の場合は、先に Rust を入れる:

```powershell
winget install Rustlang.Rustup
```

新しい PowerShell を開き直してから、上の `cargo install` を実行する。

## まず使う

インストール後、まずバージョン、記事取得、候補探索を確認する。以下の操作には翻訳 agent や GitHub 認証を必要としない。

```bash
article-collector --version
article-collector article collect 'https://news.ycombinator.com/item?id=42575537'
article-collector article collect https://news.ycombinator.com/ --discover
```

PR作成まで実行する場合は、先に GitHub CLI と翻訳 agent を認証する。初回確認では
自動 merge を無効にする。

```bash
gh auth login
codex login
export ACP_AGENT="codex"
export TARGET_REPO="your-org/your-repo"
export AUTO_MERGE="false"
article-collector article collect 'https://news.ycombinator.com/item?id=42575537' --through pr
```

Windows PowerShellでは環境変数の設定と実行ファイルの呼び出しを次のように書く:

```powershell
$env:ACP_AGENT = "codex"
$env:TARGET_REPO = "your-org/your-repo"
$env:AUTO_MERGE = "false"
& "$env:USERPROFILE\bin\article-collector.exe" article collect "https://news.ycombinator.com/item?id=42575537" --through pr
```

## 処理フロー

`article collect <URL> --through pr` は取得から PR 作成までを実行する。

```text
article-collector article collect URL --through pr
  -> fetch URL -> raw.json
  -> ACP agent translate -> translated.md
  -> target repository: prepare article branch
  -> save Markdown
  -> commit / push / PR
  -> AUTO_MERGE=true の場合は PR を merge
  -> User: PR または merged article
```

> **注意:** `AUTO_MERGE=true` は作成した PR を自動で merge する。動作確認や
> 初回導入では `AUTO_MERGE=false` を指定する。

## 機能別の使い方

### 記事の取得と翻訳

記事 URL を指定すると、本文を取得して `raw.json` に保存する。`--through translate`
を追加すると、翻訳結果も `translated.md` に保存する。

```bash
article-collector article collect 'https://news.ycombinator.com/item?id=42575537'
article-collector article collect 'https://news.ycombinator.com/item?id=42575537' --through translate
```

### X recommendation plugin

core binary のインストール後、official signed registry から recommendation
source plugin を追加できる。

```bash
article-collector plugin install x
article-collector plugin list
export X_OAUTH_CLIENT_ID="your-public-x-client-id"
article-collector plugin configure x
article-collector plugin doctor x
article-collector article collect https://x.com/ --discover
```

`x` plugin は [latest release](https://github.com/rurusasu/article-collector/releases/latest)
の release artifact と signed registry から install できる。OAuth credential の取り扱いは
[X OAuthとplugin secretの設定](docs/plugins/secret.md) に記載する。
`plugin update x`、`plugin uninstall x` も同じ CLI で実行できる。

`search` は非空の `query` を必須とし、`timeline` / `bookmarks` は query を受け付けない。
いずれも `limit` までページングし、同じ post ID は重複排除する。X API の利用料・rate
limit・bookmark permission は X Developer Platform の契約に従う。

plugin は MCP stdio server として別 process で動作する。署名と checksum は
配布物の publisher/integrity を検証するが、完全な OS sandbox ではないため、
plugin は実行ユーザーと同じ filesystem/network 権限を持つ。install 元は binary
に固定された official registry のみで、任意 URL や local executable は指定
できない。timeout 時は専用 process group/Job Object を終了するが、同一ユーザー
権限の別 process による data root 改変まで隔離するものではない。

MCP protocolとtrust boundaryは [MCP plugin](docs/plugins/README.md)、registry・
release・refreshなどplugin配布の運用は [Plugin 配布](docs/plugins/distribution.md) を
参照する。secretの値やredaction規則は [X OAuthとplugin secretの設定](docs/plugins/secret.md)
にまとめている。

### 記事候補の探索と処理

`--discover` を付けると、URL を起点として記事候補を探す。既定で候補一覧を
`raw.json` に保存する。`--through raw` は各候補の本文取得まで進み、
`--through translate` は取得した本文を翻訳する。

```bash
article-collector article collect https://news.ycombinator.com/ --discover
article-collector article collect https://zenn.dev/feed --discover --through raw
article-collector article collect https://example.com/links --discover --through translate
```

カレントディレクトリの `article-collector.toml` で探索件数と source 固有の設定を
指定できる。`[recommend].limit` は既定件数、`[recommend.source.<site>].limit` は
source ごとの件数を設定する。Qiita API v2 items search と Bluesky の public AppView
`app.bsky.feed.searchPosts` は認証なしで利用でき、検索条件はそれぞれ
`[recommend.source.qiita].query` と `[recommend.source.bluesky].query` で指定する。

`--through translate` を指定すると、`ARTICLE_COLLECTOR_TEMP_DIR` 配下に記事別 JSON
と翻訳 Markdown、記事別の一覧をまとめた `translated.md` が作成される。

```text
raw.json
translated.md
recommended_articles/
  001-hackernews-example-title.json
  001-hackernews-example-title_translated.md
recommend-fetch-failures.json
```

`translated.md` は結合本文ではなく、Slack などに貼りやすい記事別 index として作成される。各 item には title, URL, source 固有の metadata, `Summary`, `Translation` link を出力する。`Summary` は翻訳済み Markdown の最初の本文抜粋を優先し、取得できない場合は source の `summary` / `description` / `content` / `article_content` から短い抜粋を作る。

翻訳時に生成される `translated.md` の例:

```md
# Recommended Articles

Generated from `https://news.ycombinator.com/`.

## Articles

1. San Diego Photologs from the 1970s
   URL: https://www.beautifulpublicdata.com/san-diego-photologs-from-the-1970s/
   Hacker News: https://news.ycombinator.com/item?id=48647823
   Score: 43
   Comments: 7
   Source: hackernews
   Summary: 1970年代のサンディエゴで撮影された公共データ由来の写真記録を紹介する記事。街路、建物、日常風景の記録から、当時の都市の姿を読み取れる。
   Translation: recommended_articles/001-hackernews-san-diego-photologs-from-the-1970s_translated.md

2. Practical architecture note
   URL: https://martinfowler.com/articles/example.html
   Source: martinfowler
   Summary: ソフトウェア設計上の判断を小さく保ち、変更しやすい境界を作るための実践的な考え方をまとめている。
   Translation: recommended_articles/002-martinfowler-practical-architecture-note_translated.md
```

Slack に手動投稿する場合は、生成後の `translated.md` をそのままコピーする:

```powershell
$env:ARTICLE_COLLECTOR_TEMP_DIR = "$PWD/.tmp/recommend-slack"
$env:ACP_AGENT = "codex"
article-collector article collect https://news.ycombinator.com/ --discover --through translate
Get-Content "$env:ARTICLE_COLLECTOR_TEMP_DIR/translated.md" -Raw
```

Slack Incoming Webhook へ投稿する場合は、`SLACK_WEBHOOK_URL` を設定して `translated.md` 全体を text として送る:

```powershell
$message = Get-Content "$env:ARTICLE_COLLECTOR_TEMP_DIR/translated.md" -Raw
Invoke-RestMethod `
  -Method Post `
  -Uri $env:SLACK_WEBHOOK_URL `
  -ContentType "application/json" `
  -Body (@{ text = $message } | ConvertTo-Json)
```

Discord, Teams, GitHub comment など Markdown を受け取れる場所にも同じ `translated.md` を貼り付けられる。投稿前に長さを抑えたい場合は、`[recommend.source.<site>].limit` で source ごとの件数を減らす。

本文の取得や翻訳に失敗した URL は `recommend-fetch-failures.json` に `url`、`title`、
`stage`、`error` とともに記録される。翻訳した記事は SQLite の既読履歴に記録する。
PDF は本文取得の対象外で、`stage: "unsupported_pdf"` として記録される。

候補記事を翻訳し、保存先リポジトリの一つの PR にまとめるには `--through pr` を指定する。

```bash
export ACP_AGENT="codex"
export TARGET_REPO="your-org/your-repo"
article-collector article collect https://news.ycombinator.com/ --discover --through pr
```

翻訳済み記事が 0 件の場合は、保存先の clone やブランチ作成に進まず終了する。
PR に含める Markdown は、保存先リポジトリの `SAVE_PATH_TEMPLATE` に従って配置する。

PR 作成用の target repo は `TARGET_DIR` を再利用する前に git work tree として検証する。壊れた `.git` だけが残っている場合など、既存 `TARGET_DIR` が無効な場合は `TARGET_DIR.broken-*` に退避してから再 clone する。clone は sibling の短い process-specific staging directory（通常は `.c<PID>`、既存なら `.c<PID>-N`）に作成し、有効な git work tree と確認できた場合だけ `TARGET_DIR` に昇格する。`git` / `gh` の外部コマンドは 5 分で timeout し、失敗時は非ゼロ終了のまま扱う。

収集・翻訳中の一時ディレクトリと最終成果物の保存先を分ける場合は、`ARTICLE_COLLECTOR_TEMP_DIR` を temporary directory として使い、`ARTICLE_COLLECTOR_OUTPUT_DIR` に最終成果物の保存先を指定する。`ARTICLE_COLLECTOR_OUTPUT_DIR` が設定されている場合、保存ステップ後に target repo 内の `*-news` directory 配下を走査し、`.md` files のみを相対パスを保って最終成果物ディレクトリへコピーする。JSON など Markdown 以外の files や `*-news` 以外の directory はコピーしない。

## コマンド一覧

`article-collector article collect <URL> [--discover] [--through raw|translate|pr]`
の `<URL>` には HTTP または HTTPS の URL を指定する。`--discover` は候補探索を
有効にするフラグで、値を取らない。

| `--through` | 記事 URL を直接指定 | `--discover` を指定 |
| --- | --- | --- |
| 省略 | 記事本文を `raw.json` に保存 | 候補一覧を `raw.json` に保存 |
| `raw` | 記事本文を `raw.json` に保存 | 候補を探し、各記事の本文を取得 |
| `translate` | 取得・翻訳して `translated.md` を作成 | 候補記事の翻訳と一覧の Markdown を作成 |
| `pr` | 取得・翻訳し、作業ブランチに配置して PR を作成 | 候補記事を取得・翻訳し、まとめて PR を作成 |

| コマンド | 動作 | 出力または副作用 |
| --- | --- | --- |
| `article-collector history clear [--config PATH]` | 推薦履歴を削除 | 次回の探索で既読記事を再取得できる |
| `article-collector history clear --include-artifacts [--config PATH]` | 推薦履歴と成果物を削除 | 環境変数で設定済みの一時ディレクトリと出力ディレクトリを削除 |
| `article-collector plugin install <NAME[@VERSION]>` | official signed registry から compatible plugin を install | artifact の byte length/SHA-256 と MCP descriptor を検証後、version を atomic activate |
| `article-collector plugin list` | installed plugin を名前順で表示 | plugin 名、version、source を表示 |
| `article-collector plugin update <NAME>` | latest compatible version へ update | current version と同じ場合は no-op |
| `article-collector plugin configure <NAME>` | OAuth など plugin 所有の対話設定を実行 | 対応 plugin の `article_collector_configure` を呼ぶ |
| `article-collector plugin doctor <NAME>` | manifest/runtime と local staging を診断 | source descriptor mismatch、process error、stale `.staging-*` を報告 |
| `article-collector plugin uninstall <NAME>` | exact plugin directory を削除 | アプリ内から復元不可。必要なら registry から再 install |

## 設定

| 環境変数 | 必要な操作 | Default | 説明 |
|----------|----------|---------|-------------|
| `ARTICLE_COLLECTOR_TEMP_DIR` | `article collect` | OS 依存 | `raw.json`、`translated.md`、`recommended_articles/` などの一時ディレクトリ |
| `ARTICLE_COLLECTOR_OUTPUT_DIR` | `--through pr` | - | 保存先 repo の `*-news` 配下にある Markdown のコピー先 |
| `ACP_AGENT` | `--through translate`、`--through pr` | - | 翻訳用の ACP agent を `codex`、`gemini`、`claude` から選ぶ |
| `X_OAUTH_CLIENT_ID` | `plugin configure x` | - | X Developer Platform の public OAuth 2.0 PKCE client ID。[設定方法](docs/plugins/secret.md) |
| `TRANSLATE_LANG` | `--through translate`、`--through pr` | `ja` | 翻訳先言語コード |
| `TARGET_REPO` | `--through pr` | - | 保存先 GitHub リポジトリ（`owner/repo`） |
| `TARGET_DIR` | `--through pr` | OS 依存 | 保存先リポジトリのローカル clone 先 |
| `ARTICLE_COLLECTOR_GH_BIN` | `--through pr` | `gh` | PR 作成に使う GitHub CLI 実行ファイルのパス |
| `SAVE_PATH_TEMPLATE` | `--through pr` | `articles/${TYPE}/` | 保存先パステンプレート |
| `AUTO_MERGE` | `--through pr` | `true` | `true` の場合は PR 作成後に merge。初回は `false` を推奨 |

> **削除操作:** `history clear --include-artifacts` は、設定済みの
> `ARTICLE_COLLECTOR_TEMP_DIR` と `ARTICLE_COLLECTOR_OUTPUT_DIR` を、配下のファイルごと削除する。

### TOML config

`article collect --discover` と `history clear` は、カレントディレクトリの
`article-collector.toml` を自動検出する。`history clear` では `--config <PATH>` で
別のファイルを指定できる。

以前の設定から移行する場合は、`[recommend]` の `sources`、`fetch_articles`、`create_pr`
を削除する。探索元は起点 URL、到達点は `--through` で指定する。

X pluginのcallback URL、OAuth scope、tokenの保存場所は
[X OAuthとplugin secretの設定](docs/plugins/secret.md)に記載する。

plugin source 固有設定は、signed manifest が宣言する JSON Schema に従って
`options` 配下へ書く。plugin source では `query`、built-in source では
`options` を使えない。

```toml
[recommend.source.twitter]
limit = 10

[recommend.source.twitter.options]
mode = "search"
query = "(AI OR Rust OR security) lang:en -is:retweet"
```

```toml
[recommend]
limit = 30
# history_path = "D:/article-collector-data/recommend-history.sqlite"

[recommend.source.arxiv]
limit = 10
query = "cat:cs.AI OR cat:cs.CL OR cat:cs.CV OR cat:cs.LG OR cat:cs.IR OR cat:cs.SE OR cat:stat.ML"

# X plugin source（plugin install x 後に有効化）
# [recommend.source.twitter]
# limit = 10
# [recommend.source.twitter.options]
# mode = "search"
# query = "(AI OR Rust OR security) lang:en -is:retweet"

[recommend.source.qiita]
limit = 10
query = "AI OR Rust OR security"

[recommend.source.bluesky]
limit = 10
query = "AI OR Rust OR security"

[recommend.source.github-advisory]
limit = 10

[recommend.source.cisa-kev]
limit = 10

[recommend.source.nvd]
limit = 5

[recommend.source.github-search]
limit = 5
query = "stars:>1000 pushed:>2026-01-01 archived:false"
```

探索件数は source 別の `limit`、`[recommend].limit`、組み込みの既定値の順に適用する。
X plugin の mode と query は `[recommend.source.twitter.options]` に記述し、検索対応の
built-in source では source 別の `query` を使う。処理範囲は CLI の `--through` で選ぶ。

### ACP agent 利用時の注意

`article-collector` は `ACP_AGENT` の内蔵 mapping で解決した agent を subprocess として起動し、ACP v1 の `initialize`、`session/new`、`session/prompt` を使う。
article-collector は翻訳用途の client として動作するため、file system / terminal capability は広告しない。
agent から `session/request_permission` が来た場合は `cancelled` で応答する。

内蔵 mapping:

| `ACP_AGENT` | Command |
|-------------|---------|
| `codex` | `npx -y @agentclientprotocol/codex-acp` |
| `gemini` | `gemini --acp` |
| `claude` | `npx -y @agentclientprotocol/claude-agent-acp` |

共有サービス、本番バッチ、チームでの継続的な自動化では、利用する ACP agent の認証方式、課金、利用規約を確認する。

参考:

- [Agent Client Protocol](https://agentclientprotocol.com/)
- [ACP agents](https://agentclientprotocol.com/get-started/agents)

## 対応 URL と保存分類

取得ルートと保存時の `${TYPE}` 分類は別に判定する。

| 対象 URL | 取得方法 | 保存 `${TYPE}` | Auth / 制約 |
|----------|----------|----------------|-------------|
| Dev.to | Dev.to public API | `web` | None |
| X/Twitter status (`x.com` / `twitter.com`) | Syndication API | `x` | Direct status fetch remains built-in; recommendation source is optional `x` plugin with OAuth 2.0 PKCE/read-only scopes |
| Qiita | generic web fetch | `web` | Recommend source uses Qiita API v2 items search |
| Bluesky post (`bsky.app/profile/.../post/...`) | generic web fetch | `web` | Recommend source uses public AppView searchPosts |
| YouTube (`youtube.com/watch` / `youtu.be`) | oEmbed + transcript fetch | `youtube` | None |
| Hacker News item | Firebase public API | `web` | None |
| Zenn | generic web fetch | `web` | None |
| arXiv | generic web fetch | `paper` | None |
| GitHub Advisory (`github.com/advisories`) | generic web fetch | `web` | Public API rate limit applies to recommend collection |
| CISA KEV / NVD CVE (`nvd.nist.gov/vuln/detail`) | generic web fetch | `web` | CISA catalog items use CVE-specific NVD URLs for dedupe |
| AWS official pages | generic web fetch | `web` | RSS recommend sources: What's New, Security Bulletins |
| Google Cloud Blog | generic web fetch | `web` | RSS recommend source |
| Kubernetes Blog | generic web fetch | `web` | RSS recommend source |
| CNCF Blog | generic web fetch | `web` | RSS recommend source |
| InfoQ | generic web fetch | `web` | RSS recommend source |
| Martin Fowler | generic web fetch | `web` | Atom recommend source |
| GitHub repositories (`github.com/<owner>/<repo>`) | generic web fetch | `web` | GitHub Search API rate limit applies to recommend collection |
| Thoughtworks Radar | generic web fetch | `web` | Manual URL fallback only; stable machine-readable Radar feed/API is not confirmed |
| DOI (`doi.org`) | generic web fetch | `paper` | None |
| OpenReview | generic web fetch | `paper` | None |
| その他の `http://` / `https://` URL | generic web fetch | `web` | None |

### 探索 URL と履歴

組み込み discovery endpoint に対応する URL は、その source の API や feed を使う。
例として、Hacker News の `https://news.ycombinator.com/`、Zenn の
`https://zenn.dev/feed`、arXiv の `https://export.arxiv.org/api/query` がある。
X の `https://x.com/` と `https://twitter.com/` からの探索は、インストール済みの
`x` plugin を使う。その他の Web ページでは、ページ内の HTTP(S) リンクをたどる候補を抽出する。

候補一覧の出力と翻訳済み記事は SQLite の既読履歴に記録する。履歴 DB は既定で
ユーザー設定ディレクトリ配下の `article-collector/recommend-history.sqlite` にあり、
`[recommend].history_path` で場所を変更できる。候補が見つからない場合や全候補が
既読の場合は非ゼロで終了する。

再収集したい場合は `article-collector history clear` を実行する。成果物も削除する
場合は `--include-artifacts` を追加する。ディレクトリを指定する環境変数が未設定なら、
既定の一時ディレクトリは削除対象から外れる。

## 詳細ドキュメント

| 読者 | 内容 |
|------|------|
| plugin 利用者 | [Plugin 配布](docs/plugins/distribution.md)：registry、install/update、release/refresh、production smoke test |
| plugin 実装者 | [MCP plugin](docs/plugins/README.md)：MCP protocol、trust boundary、local state、recommendation接続 |
| secret を設定する利用者 | [X OAuthとplugin secretの設定](docs/plugins/secret.md)：callback、OAuth scope、token保存、redaction |
| CI・release の運用者 | [CI/CD documentation](docs/ci-cd/README.md)：release publisher、署名済みregistry、refresh workflow |
| repository の開発者 | [docs/AGENTS.md](docs/AGENTS.md)：ドキュメント構成と更新ルール |

## 開発

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
taplo format --check Cargo.toml article-collector.toml .config/release-plz.toml
taplo lint Cargo.toml article-collector.toml .config/release-plz.toml
cargo test --locked
cargo build --release --locked
```

CLI の記事取得と候補探索は、`cargo test --locked --test cli` でローカル HTTP
サーバーを使って検証できる。

Taskfile は Rust CLI の薄いラッパーとして使える:

```bash
task collect URL=https://example.com/article
task collect URL=https://news.ycombinator.com/ DISCOVER=true THROUGH=translate
task check
```

## リリース

CLI の切り替えにあわせ、バッチやスクリプトは新しい [コマンド一覧](#コマンド一覧)
に従って更新する。探索の処理範囲は `--through` で指定する。

`main` に push されると `.github/workflows/release.yml` が実行される。
通常の PR merge では release-plz が Release PR を作成・更新する。Release PR が merge されると workflow が `Cargo.toml` の version から `v<major>.<minor>.<patch>` 形式の tag を解決し、GitHub Release、core binary、`article-collector-x` plugin binary を同じ workflow 内で作成する。5 target の build が成功した後、plugin registry も更新する。

release-plz は `git_only = true` で動かすため、Release PR の version 判定は git tag を基準にする。tag は `v<major>.<minor>.<patch>` 形式を使う。既存 tag に対して release workflow を手動実行しても、同じ release を再作成せず、次の version bump または registry refresh を待つ。

詳細: [docs/ci-cd/README.md](docs/ci-cd/README.md)

## トラブルシューティング

### `cargo` が見つからない

Rust toolchain をインストールしてから、新しい shell を開き直す。

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### リリースのダウンロードが失敗する

asset 名が platform と一致しているか、latest release に対象 asset があるかを確認する。
急ぐ場合や対象 platform の asset がない場合は、`cargo install --git https://github.com/rurusasu/article-collector --locked` を使う。

### `gh article-collector` として使いたい

現在の構成は通常の CLI (`article-collector`) 用。
GitHub CLI extension として `gh article-collector ...` で使うには、別途 `gh-article-collector` 名の実行ファイルまたは GitHub CLI extension 用の repo 構成が必要。

## ライセンス

MIT
