# Qiita

## サイトの識別情報

- サイト名: `qiita`
- 別名: `qiita.com`
- 対応記事 URL:
  - `https://qiita.com/<user>/items/<id>`

## URL 構造

Qiita の記事 URL は `qiita.com/<user>/items/<id>` を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::QiitaItems`
- endpoint: `https://qiita.com/api/v2/items`
- query: `[recommend.source.qiita].query`

Qiita API v2 の public items endpoint を使い、`page=1`, `per_page`, `query` で記事候補を取得する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

fetch は direct article URL を generic web fetch に渡す。Qiita API は recommendation discovery のみに使う。
