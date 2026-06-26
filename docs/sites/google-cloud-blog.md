# Google Cloud Blog

## サイトの識別情報

- サイト名: `google-cloud-blog`
- 別名: `google-cloud`, `gcp`
- 対応記事 URL: `https://cloud.google.com/blog/<slug>`

## URL 構造

Google Cloud blog post は `cloud.google.com/blog/` 配下にある。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://cloudblog.withgoogle.com/rss`

discovery は RSS item を parse し、article candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

blog page は `FetchRoute::GenericWeb` で取得する。

## 補足

feed host と article host が異なるため、site entry が両方をまとめて保持する。
