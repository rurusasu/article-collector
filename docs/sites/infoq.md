# InfoQ

## サイトの識別情報

- サイト名: `infoq`
- 別名: `infoq-news`, `architecture-news`
- 対応記事 URL: `https://www.infoq.com/<path>/`

## URL 構造

InfoQ content は `www.infoq.com` 配下で、news、article、presentation など複数の path family を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://feed.infoq.com/`

discovery は RSS item を parse し、candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

InfoQ page は `FetchRoute::GenericWeb` で取得する。

## 補足

site の URL structure が広いため、fetch rule は意図的に `infoq.com/` に match させる。
