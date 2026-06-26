# DOI

## サイトの識別情報

- サイト名: `doi`
- 別名: `doi.org`
- 対応記事 URL: `https://doi.org/<doi>`

## URL 構造

DOI URL は `doi.org` 経由で解決され、publisher page に redirect される場合がある。

## discovery endpoint の構造

- 種類: なし

DOI は discovery source ではなく、直接 URL fetch と save classification のための entry である。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Paper`

DOI URL は `FetchRoute::GenericWeb` で取得し、`SaveType::Paper` として保存する。

## 補足

discovery は DOI 自体ではなく、通常 arXiv や OpenReview のような paper index 経由で行う。
