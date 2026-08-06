# X MCP Recommendation Plugin Design

> **Status (2026-08-06):** この設計は実装済み。X plugin は core `v0.6.23` の 5
> target artifact と signed production registry から導入できる。issue #85 は設計時の
> tracking reference としてのみ残し、現行仕様は [../../sites/twitter.md](../../sites/twitter.md)
> と [../../plugins/README.md](../../plugins/README.md) を参照する。

## Goal

X/Twitter の recommendation discovery を本体から分離し、ユーザーが
`article-collector plugin install x` で追加できる `article-collector-x`
connector として提供する。既存の direct tweet URL fetch は維持し、X 固有の
OAuth、hosted MCP 通信、response schema は connector process に閉じ込める。

## Scope and constraints

- plugin source は `twitter`、aliases は `x` と `x-twitter` とする。
- options の `mode` は必須で、`search`、`timeline`、`bookmarks` のいずれかに限定する。
- `search` は非空 `query` を必須とし、`timeline` / `bookmarks` は `query` を拒否する。
- OAuth 2.0 Authorization Code with PKCE を使い、scopes は `tweet.read users.read bookmark.read offline.access` のみとする。
- write scope は要求せず、投稿・like・bookmark 変更などの tool は呼び出さない。
- access/refresh token、authorization code、client credential は core に返さず、通常ログにも出さない。
- X hosted MCP の endpoint は `https://api.x.com/mcp`、必要 tool 名は issue #85 の contract に固定する。`tools/list` で存在と入力 schema を検証し、差異があれば fail closed とする。
- 5 target は既存 plugin target と同じ Linux amd64/arm64、Windows amd64、macOS amd64/arm64 とする。

## Chosen architecture

既存 package に次の binary target を追加する。

```text
article-collector (core)
  ├─ installed plugin manifest/source catalog
  └─ stdio MCP client
       └─ article-collector-x (separate process)
            ├─ stdio MCP server for article-collector
            ├─ OAuth PKCE and local token store
            └─ Streamable HTTP MCP client
                 └─ https://api.x.com/mcp
```

`src/x_connector/` は protocol-independent な domain logic、OAuth/token store、
remote MCP adapter、stdio server を分割して持つ。`src/bin/article-collector-x.rs`
は process entry point だけを担当し、integration tests は shared module を直接
利用できるよう `src/lib.rs` から公開する。

この方式を採用する理由は、registry が現在の core release asset URL を使うため、
plugin 専用 repository の release coordination を増やさずに installable artifact
を作れること、core binary に X OAuth code をリンクせず process boundary を維持
できること、既存の `PluginRuntime` protocol と 5 target mapping を再利用できる
ことである。

## Data flow

### Plugin lifecycle

1. `plugin install x` は signed manifest、artifact byte length/SHA-256、runtime descriptor を検証する。
2. `plugin configure x` は connector の `article_collector_configure` を呼ぶ。
3. connector は client ID と固定 loopback redirect URI を読み、state と PKCE verifier を生成する。
4. authorization URL を stderr に表示し、loopback callback で code/state を受ける。
5. code を token endpoint へ交換し、token を user-data 配下の mode `0600` file に atomic write する。
6. access token で remote MCP initialize、`tools/list`、`getUsersMe` を実行して接続を確認する。

client ID は signed manifest が許可する `X_OAUTH_CLIENT_ID` 環境変数から読む。client
secret は使わない public PKCE client とし、secret を plugin/Core の設定へ導入しない。
redirect URI は既定の `http://127.0.0.1:53681/callback` とし、X Developer Portal
への登録手順を docs に記載する。

### Collection

毎回の collect で token store を読み、期限切れまたは remote MCP から 401 を受けた
場合は refresh token を使って一度だけ refresh し、token store を置換する。
remote session は initialize と `tools/list` を行った後、次の順に tool を呼ぶ。

| mode | tool sequence | arguments |
| --- | --- | --- |
| `search` | `searchPostsRecent` | `query`, `max_results`, tweet/user expansion fields, optional `next_token` |
| `timeline` | `getUsersMe` → `getUsersTimeline` | user id, `max_results`, expansion fields, optional `next_token` |
| `bookmarks` | `getUsersMe` → `getUsersBookmarks` | user id, `max_results`, expansion fields, optional `next_token` |

The connector requests only read operations and checks each tool response for the expected
X API envelope. It follows `next_token` until the requested limit is reached or the token
is exhausted, with a page-count guard. Post IDs are deduplicated before normalization.

### Candidate normalization

Each post is mapped to the plugin candidate contract:

- `title`: first non-empty text line, bounded to a readable title length.
- `content`: post text.
- `url`: `https://x.com/<username>/status/<id>` when the author expansion has a username; otherwise `https://x.com/i/web/status/<id>`.
- `metadata`: `author`, `username`, `created_at`, `tweet_id`, and `public_metrics` when present.

`source`、`site`、`rank` は plugin response に含めず、core が既存 pipeline で付与する。
Malformed post data is skipped only when it lacks optional fields; missing id/text or an
invalid envelope is an actionable error so a partial, misleading source result is not
silently accepted.

## Error handling and security

- missing client ID or missing token: actionable message naming the required configuration;
- state mismatch, callback timeout, OAuth denial, invalid grant: configuration failure without code/token values;
- 401: one refresh retry, then a revoked/expired-token error;
- 403: scope/permission error;
- 429: rate-limit error including reset time when supplied, never token values;
- missing/changed required tool: incompatible hosted MCP error, no direct API fallback;
- malformed MCP/JSON response: bounded diagnostic with response body omitted or truncated;
- transport timeout and unexpected status: status plus endpoint class, not authorization header.

Token files are written below the same OS local-data root used by article-collector, under
`x/credentials.json`, with restrictive permissions on Unix and atomic replacement. The
connector never prints token contents. The core manifest allows only `X_OAUTH_CLIENT_ID`
and the plugin process still receives a cleared environment for all other variables.

## Testing strategy

Tests are written before production code and run in red/green cycles.

1. Pure validation tests cover mode/query rules, descriptor/options schema, tool allowlist, and required scopes.
2. Pure normalization tests cover author expansion, canonical URLs, duplicate post IDs, pagination limit, and malformed envelopes.
3. A fake Streamable HTTP MCP server records initialize/tools/list/tools/call and returns deterministic search/timeline/bookmark pages, 401-refresh, 403, 429, tool mismatch, and malformed-response fixtures.
4. OAuth tests use a local fake authorization/token endpoint and verify PKCE challenge/state, refresh, atomic token persistence, and secret redaction.
5. The stdio server is exercised through the same JSON-RPC framing used by `PluginRuntime`, including descriptor, configure, and collect responses.
6. Existing core tests verify `x.com`/`twitter.com` direct status URLs still route to `SocialStatus`, while `twitter` is no longer a built-in discovery endpoint.
7. Release checks build all five connector targets and validate registry JSON/manifest/checksums. A real-account smoke test remains manual and must never record credentials.

## Release and documentation

- Update release workflow to build/package `article-collector-x` for all five targets and upload assets before registry publication.
- Add the signed `x` manifest and artifact checksums to the registry only after all assets exist.
- Remove the old X recent-search code, environment-token docs, and obsolete tests/dependencies; preserve direct status fetch docs.
- Document client ID setup, loopback redirect registration, OAuth scopes, three modes, query rules, pagination semantics, and X API usage/cost considerations.
- Keep `registry/plugins-v1.json` and its signature changes artifact-first; do not claim `plugin install x` works until published assets and signed registry are available.

## Alternatives considered

### Separate repository for `article-collector-x`

This gives independent versioning and a smaller plugin repository, but needs a second release
pipeline and registry asset lifecycle before the first install can work. It is deferred until
there is a second production plugin or independent release cadence.

### Keep X discovery in core and add a thin plugin wrapper

This minimizes immediate code movement, but violates the required trust boundary because the
core still handles X token and response schema. It is rejected.

## External specification note

X official authentication documentation confirms OAuth 2.0 PKCE and the read scopes used here,
including `bookmark.read` and `offline.access`. The hosted MCP endpoint/tool contract is tracked
by issue #85 and is validated at runtime with `tools/list`; a live smoke test is required before
registry publication because hosted MCP descriptors may evolve independently of the core release.
