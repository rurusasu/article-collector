# Bluesky

## サイトの識別情報

- サイト名: `bluesky`
- 別名: `bsky`, `bsky.app`
- 対応記事 URL:
  - `https://bsky.app/profile/<handle>/post/<rkey>`

## URL 構造

Bluesky の web post URL は `bsky.app/profile/<handle>/post/<rkey>` を使う。recommend discovery では AT URI の最後の segment を `rkey` として使い、author handle と組み合わせて web URL を作る。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::BlueskySearchPosts`
- endpoint: `https://public.api.bsky.app/xrpc/app.bsky.feed.searchPosts`
- query: `[recommend.source.bluesky].query`

public AppView の `app.bsky.feed.searchPosts` を使う query-based recommendation source として扱う。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

fetch は web post URL を generic web fetch に渡す。専用 social fetch route や authenticated timeline collection は対象外。
