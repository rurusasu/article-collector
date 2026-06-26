# CNCF Blog

## サイトの識別情報

- サイト名: `cncf`
- 別名: `cloud-native`, `cncf-blog`
- 対応記事 URL: `https://www.cncf.io/blog/<yyyy>/<mm>/<dd>/<slug>/`

## URL 構造

CNCF blog post は `/blog/` 配下の日付付き path を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://www.cncf.io/feed/`

discovery は RSS item を parse し、article candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

CNCF page は `FetchRoute::GenericWeb` で取得する。

## 補足

feed には blog post 以外が含まれることがある。candidate normalization では source site name を保持する。
