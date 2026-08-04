# X MCP Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** X/Twitter recommendation discovery を `article-collector-x` の signed MCP plugin に分離し、OAuth 2.0 PKCE で `search`、`timeline`、`bookmarks` を収集できるようにする。

**Architecture:** 既存 package に `article-collector-x` binary target と `src/x_connector/` shared module を追加する。core は direct X recent-search と Bearer Token を削除し、既存 plugin MCP client から connector を呼ぶ。connector は OAuth/token store、X hosted Streamable HTTP MCP client、article-collector 向け stdio MCP server を所有する。

**Tech Stack:** Rust stable 2021、Tokio、Reqwest/rustls、rmcp 2.2 Streamable HTTP client、serde/serde_json、url、base64、sha2、rand、既存 signed registry/release workflow。

## Global Constraints

- source 名は `twitter`、aliases は `x` と `x-twitter` とする。
- `mode` は必須で `search` / `timeline` / `bookmarks` のいずれかだけを許可する。
- `search` は非空 `query` 必須、`timeline` / `bookmarks` は `query` を拒否する。
- OAuth scopes は `tweet.read users.read bookmark.read offline.access` のみとし、write scope を要求しない。
- X hosted MCP endpoint は `https://api.x.com/mcp` とし、`tools/list` で required tool と schema を検証する。差異時の direct API fallback は禁止する。
- 5 target は `x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu`、`x86_64-pc-windows-msvc`、`x86_64-apple-darwin`、`aarch64-apple-darwin` とする。
- token、refresh token、authorization code、client credential は core response/log に出さない。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`、`cargo build --release --locked` を完了条件とする。
- Plane はユーザー指示により更新しない。

## File Map

- Create: `src/lib.rs`, `src/x_connector/{mod,normalize,oauth,token_store,remote_mcp,stdio}.rs`, `src/bin/article-collector-x.rs`, `tests/x_connector.rs`
- Modify: `src/recommend.rs`, `src/sites/registry.rs`, `src/discovery/types.rs`, `src/discovery/planner.rs`
- Modify: `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/sites/twitter.md`, `docs/AGENTS.md`, `article-collector.toml`
- Create: `registry/x-plugin-manifest.json`, `scripts/generate-x-plugin-registry.sh`
- Modify: `.github/workflows/release.yml`, `scripts/verify-plugin-registry.sh`

---

### Task 1: Core の direct X discovery を plugin-only にする

**Interfaces:** `twitter`、`x`、`x-twitter` は installed plugin source だけで解決し、x.com/twitter.com status URL は引き続き `FetchRoute::SocialStatus` へ解決する。

- [ ] **Step 1: Write the failing tests**

既存の X recommendation test を、built-in registry に `XRecentSearch` endpoint がなく、plugin catalog が空なら `plugin install x` を案内して拒否する期待値へ変更する。direct status URL route test は残す。

- [ ] **Step 2: Verify red**

```sh
cargo test --locked recommend::tests::resolves_twitter_site_name_to_recent_search
cargo test --locked --test architecture
```

Expected: current built-in X discovery が残るため、変更した test が失敗する。

- [ ] **Step 3: Implement the minimal removal**

`SearchRequest::XRecentSearch`、`recommend.rs` の X recent-search collector/Bearer Token helper/parser、対応する tests/docs を削除する。site name/aliases、URL matcher、SocialStatus route、direct fetch normalization は変更しない。

- [ ] **Step 4: Verify green**

```sh
cargo fmt --check
cargo test --locked recommend::tests
cargo test --locked --test architecture
rg -n "X_BEARER_TOKEN|TWITTER_BEARER_TOKEN|XRecentSearch" src
```

Expected: focused tests pass and the final search returns no production match.

- [ ] **Step 5: Commit**

```sh
git add src/recommend.rs src/sites/registry.rs src/discovery/types.rs src/discovery/planner.rs
git commit -m "refactor: move X recommendation discovery to plugin"
```

### Task 2: Connector descriptor, options, and normalization

**Interfaces:** `src/x_connector/mod.rs` exports `descriptor()`, `parse_collect_options(source, options)`, `CollectionMode`、`CollectOptions`、`PluginCandidate`、`normalize_response(value)`。Descriptor は source `twitter`、aliases `x/x-twitter`、mode-required JSON Schema、configuration 対応を返す。

- [ ] **Step 1: Write failing tests**

`tests/x_connector.rs` に、descriptor identity/schema、unknown/missing mode、search query 必須、timeline/bookmarks の query 拒否、author expansion/canonical URL、fallback URL、post ID 重複排除、missing id/text/malformed envelope を検証する test を追加する。

- [ ] **Step 2: Verify red**

```sh
cargo test --locked --test x_connector options_and_descriptor
cargo test --locked --test x_connector normalizes
```

Expected: connector module がないため compile/test failure になる。

- [ ] **Step 3: Implement minimally**

`CollectionMode` を exact enum として定義し、options は object のみ・unknown key なし・mode 必須で parse する。X response の `data` と `includes.users` を検証し、first non-empty line title、content、canonical URL、author/username/created_at/tweet_id/public_metrics metadata を生成する。reserved core metadata は生成しない。

- [ ] **Step 4: Verify green**

```sh
cargo test --locked --test x_connector options_and_descriptor
cargo test --locked --test x_connector normalizes
```

- [ ] **Step 5: Commit**

```sh
git add src/lib.rs src/x_connector tests/x_connector.rs
git commit -m "feat: add X connector source contract and normalization"
```

### Task 3: OAuth PKCE and secure token storage

**Interfaces:** `TokenStore::load/save/clear` は `TokenSet { access_token, refresh_token, expires_at }` を OS local-data 配下へ atomic 保存する。`OAuthClient::authorize` と `refresh` は `OAuthConfig { client_id, authorize_url, token_url, redirect_uri }` を使い、token を error/log へ出さない。

- [ ] **Step 1: Write failing tests**

token round-trip、Unix `0600`、failed write 時の旧 file 保持、expiry 判定、PKCE S256 challenge、scope/state/redirect URI、callback state mismatch、OAuth denial を fake server で検証する。

- [ ] **Step 2: Verify red**

```sh
cargo test --locked --test x_connector token_store
cargo test --locked --test x_connector pkce
```

- [ ] **Step 3: Implement minimally**

`rand` を direct dependency に追加し、OS randomness で verifier/state を生成する。S256 は `sha2`、query/form は `url::form_urlencoded`、token endpoint は bounded error parsing を使う。staging file を sync し Unix で `0600` を設定して rename し、失敗時は active file を変更しない。

- [ ] **Step 4: Verify green and scan**

```sh
cargo test --locked --test x_connector token_store
cargo test --locked --test x_connector pkce
rg -n "access_token|refresh_token" src/x_connector
```

Expected: match は field/request/serialization 部分だけで、logging/diagnostic には token がない。

- [ ] **Step 5: Commit**

```sh
git add Cargo.toml Cargo.lock src/x_connector tests/x_connector.rs
git commit -m "feat: add X OAuth PKCE token store"
```

### Task 4: Remote Streamable HTTP MCP collection

**Interfaces:** `XRemoteMcp::validate_tools` は required read tools と input schema を検証し、`XRemoteMcp::collect(options, limit)` は normalized candidates を返す。mode tool sequence は search=`searchPostsRecent`、timeline=`getUsersMe`→`getUsersTimeline`、bookmarks=`getUsersMe`→`getUsersBookmarks`。

- [x] **Step 1: Write fake-server tests**

local fake Streamable HTTP MCP server を作り initialize/tools/list/tools/call を記録する。3 mode、`max_results`/pagination/limit、duplicate、tool mismatch、malformed response、401 refresh、403、429 reset header を test する。

- [x] **Step 2: Verify red**

```sh
cargo test --locked --test x_connector remote_mcp
```

- [x] **Step 3: Implement the adapter**

rmcp に `transport-streamable-http-client-reqwest` feature を追加する。protocol `2025-06-18` で initialize、`Bearer <token>` auth header で `tools/list` と tool calls を行い、page counter を bounded にする。401 は token refresh 後に一度だけ再試行し、direct API fallback はしない。

- [x] **Step 4: Verify green and secret redaction**

```sh
cargo test --locked --test x_connector remote_mcp
cargo test --locked --lib x_connector::remote_mcp
```

Expected: fake-server tests pass and diagnostics never contain Authorization/token fixtures.

- [x] **Step 5: Commit**

```sh
git add Cargo.toml Cargo.lock src/x_connector tests/x_connector.rs
git commit -m "feat: collect X modes through hosted MCP"
```

### Task 5: stdio MCP server and executable

**Interfaces:** `pub async fn run_stdio() -> anyhow::Result<()>` は core と同じ `JsonRpcMessageCodec<Value>` を使う。`article_collector_describe_sources`、`article_collector_configure`、`article_collector_collect` を実装し、collect response は `structuredContent: { items }` を含む。

- [ ] **Step 1: Write protocol tests**

initialize、initialized notification、tools/list、describe、invalid options、configure success/error、collect requests を handler に与え、JSON-RPC error、structuredContent、reserved metadata rejection、stdout-only MCP、secret redaction を検証する。

- [ ] **Step 2: Verify red**

```sh
cargo test --locked --test x_connector stdio
```

- [ ] **Step 3: Implement**

`FramedRead/FramedWrite<JsonRpcMessageCodec<Value>>` を max 16 MiB で使い、stdout は MCP frame 専用、authorization URL/diagnostics は stderr に出す。`src/bin/article-collector-x.rs` は Tokio entry point から `run_stdio()` だけを呼ぶ。

- [ ] **Step 4: Verify green/build**

```sh
cargo test --locked --test x_connector stdio
cargo build --locked --bin article-collector-x
```

- [ ] **Step 5: Commit**

```sh
git add src/lib.rs src/x_connector src/bin/article-collector-x.rs tests/x_connector.rs
git commit -m "feat: expose X connector over plugin MCP stdio"
```

### Task 6: Signed registry and five-target release assets

**Interfaces:** `registry/x-plugin-manifest.json` は `x` plugin、protocol compatibility、`recommend-source`、configuration、`X_OAUTH_CLIENT_ID`、source schema を宣言する。`scripts/generate-x-plugin-registry.sh <source-index> <output-index> <release-tag> <asset-dir>` は exactly five non-empty connector assets を検証し、byte length/SHA-256 と HTTPS release URL を生成する。

- [ ] **Step 1: Write generator tests**

5 target fixture artifacts、missing target、empty artifact、duplicate source owner、input payload preservation を検証する `tests/generate-x-plugin-registry.sh` を先に追加する。

- [ ] **Step 2: Verify red**

```sh
bash tests/generate-x-plugin-registry.sh
```

Expected: generator/manifest がないため失敗する。

- [ ] **Step 3: Implement workflow**

release matrix の各 runner で core と `article-collector-x` を build/package/upload する。`plugin-registry-publish` は build 成功後に connector assets を download し、generator→registry validate→protected signing key で exact-byte sign→verify→registry asset upload の順で実行する。checked-in registry は実 asset checksum が出るまで空の development fixture とする。

- [ ] **Step 4: Verify**

```sh
bash tests/generate-x-plugin-registry.sh
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
```

Expected: schema/fixture/workflow checks pass and private key is absent from tracked files.

- [ ] **Step 5: Commit**

```sh
git add registry scripts .github/workflows/release.yml
git commit -m "ci: publish X connector artifacts and registry metadata"
```

### Task 7: Documentation and final verification

**Interfaces:** README/site docs は `X_OAUTH_CLIENT_ID`、`http://127.0.0.1:53681/callback`、read-only scopes、3 modes、query/pagination/limit、cost/permission、direct status fetch 境界を説明する。old bearer variables は説明・実装から消す。

- [ ] **Step 1: Write doc/CLI assertions**

`plugin install/configure`、3 modes、exact scopes、loopback callback、direct status fetch、old bearer names absent を test/grep assertions にする。

- [ ] **Step 2: Update docs/config**

`README.md`、`docs/sites/twitter.md`、`docs/AGENTS.md`、`article-collector.toml` を plugin setup と mode options に更新し、未インストール時に built-in `recommend all` が X を含まないことを明記する。

- [ ] **Step 3: Full verification**

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
bash scripts/verify-plugin-registry.sh
bash scripts/verify-workflow-security.sh
task verify
```

Expected: all commands exit 0.

- [ ] **Step 4: Final diff/secret review**

```sh
git diff --check origin/main...HEAD
git diff --stat origin/main...HEAD
rg -n "X_BEARER_TOKEN|TWITTER_BEARER_TOKEN|Bearer [A-Za-z0-9._-]{12,}|access_token.*log|refresh_token.*log" . --glob '!target/**'
git status --short --branch
```

Expected: no old direct-token path, no credential literal, clean worktree, scoped diff only.

- [ ] **Step 5: Commit**

```sh
git add README.md docs/sites/twitter.md docs/AGENTS.md article-collector.toml tests
git commit -m "docs: document X MCP recommendation plugin"
```
