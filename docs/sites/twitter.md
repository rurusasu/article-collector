# X / Twitter

現行の recommendation plugin は core release と同じ release tag で公開される signed
`x` plugin である。初回は次で install する。

```bash
article-collector plugin install x
article-collector plugin doctor x
```

## サイトの識別情報

- サイト名: `twitter`
- 別名: `x`, `x-twitter`
- 対応記事 URL:
  - `https://x.com/<user>/status/<id>`
  - `https://twitter.com/<user>/status/<id>`

## URL と direct fetch

tweet URL は `x.com` または `twitter.com` 配下の `/status/<id>` を使う。
直接 tweet URL の取得は built-in の `FetchRoute::SocialStatus` / `SaveType::X`
で引き続き利用できる。direct fetch は X OAuth plugin の install を必要としない。

## Recommendation plugin

X の recommendation discovery は core から分離された signed `x` plugin が担当する。
core の `twitter` site entry には discovery endpoint がなく、X の recommend source は
installed plugin の manifest から解決される。
plugin を install すると、source list を指定しない `recommend all` では X も対象になる。
X を除外したい場合は、`[recommend].sources` に built-in source だけを列挙する。明示的に
X を使う場合は次を実行する。

```bash
article-collector plugin install x
export X_OAUTH_CLIENT_ID="your-public-x-client-id"
article-collector plugin configure x
```

X Developer Platform には loopback callback URL
callback URL、OAuth scope、client secretを使わないこと、tokenの保存場所は
[plugin secret guide](../plugins/secret.md)に記載する。

plugin の endpoint は X hosted MCP `https://api.x.com/mcp` で、`tools/list` によって
required read tool と input schema を検証する。tool 契約が変わった場合は fail-closed
し、direct X API fallback は行わない。

## Mode と設定

source 固有 options は TOML の `options` 配下に置く。

### Search

`search` は非空 `query` が必須。`limit` 件まで `searchPostsRecent` をページングし、
post ID の重複を除去する。

```toml
[recommend.source.twitter]
limit = 10

[recommend.source.twitter.options]
mode = "search"
query = "(AI OR Rust OR security) lang:en -is:retweet"
```

### Timeline / bookmarks

`timeline` と `bookmarks` は `query` を指定しない。connector は `getUsersMe` を先に
呼び、その user ID を使って対応する read tool をページングする。

```toml
[recommend.source.twitter]
limit = 10

[recommend.source.twitter.options]
mode = "timeline"
# mode = "bookmarks" も指定可能
```

X API の利用料、rate limit、bookmark permission は X Developer Platform の契約に
従う。401 は refresh token で一度だけ再試行し、403、429、malformed response、tool
schema 不一致は安全な診断で終了する。

公式仕様:

- [X MCP Servers](https://docs.x.com/tools/mcp)
- [Search Posts](https://docs.x.com/x-api/posts/search/introduction)
- [Bookmarks](https://docs.x.com/x-api/posts/bookmarks/introduction)
- [Timelines](https://docs.x.com/x-api/posts/timelines/introduction)

## Candidate normalization

post の最初の非空行を title、本文を content とし、author expansion があれば
`https://x.com/<username>/status/<id>`、なければ
`https://x.com/i/web/status/<id>` を canonical URL にする。metadata には
`tweet_id`、`author`、`username`、`created_at`、`public_metrics` を必要に応じて保持する。
`source`、`site`、`rank` は core 所有のため plugin は返さない。
