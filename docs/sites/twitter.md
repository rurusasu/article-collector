# X / Twitter

## サイトの識別情報

- サイト名: `twitter`
- 別名: `x`, `x-twitter`
- 対応記事 URL:
  - `https://x.com/<user>/status/<id>`
  - `https://twitter.com/<user>/status/<id>`

## URL 構造

tweet URL は `x.com` または `twitter.com` 配下の `/status/<id>` を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- endpoint: `https://api.x.com/2/tweets/search/recent`
- 認証: `X_BEARER_TOKEN` または `TWITTER_BEARER_TOKEN`
- query: `[recommend.source.twitter].query`

この discovery は X の For You timeline そのものではなく、X API v2 recent search を使った query-based recommendation source として扱う。

## article fetch の方法

- fetch route: `FetchRoute::SocialStatus`
- save type: `SaveType::X`

fetch は URL から status ID を抽出し、social status 向け metadata または fallback item を返す。

## 補足

直接 tweet URL fetch と recent search discovery は同じ `twitter` site entry で扱う。browser scraping や user-authenticated home timeline collection は対象外。
