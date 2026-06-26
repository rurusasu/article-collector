# OpenReview

## サイトの識別情報

- サイト名: `openreview`
- 別名: `openreview.net`
- 対応記事 URL: `https://openreview.net/forum?id=<id>`

## URL 構造

OpenReview forum page は `forum?id=<id>` を使う。

## discovery endpoint の構造

- 種類: なし

この site は現在、直接 URL fetch と save classification のみ対応する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Paper`

OpenReview page は `FetchRoute::GenericWeb` で取得し、`SaveType::Paper` として保存する。

## 補足

将来 OpenReview API discovery を追加する場合は、この site entry の discovery endpoint として扱う。
