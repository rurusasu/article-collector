# Dev.to

## サイトの識別情報

- サイト名: `devto`
- 別名: `dev.to`
- 対応記事 URL: `https://dev.to/<author>/<slug>`

## URL 構造

article URL は author と slug を基準にしている。public API は URL path から article metadata を解決できる。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::JsonApi`
- request: `JsonRequest::PaginatedPerPage`
- endpoint: `https://dev.to/api/articles?top=7`

discovery は `per_page=<limit>` を付与し、article JSON を article candidate へ正規化する。

## article fetch の方法

- fetch route: `FetchRoute::SiteArticleApi`
- save type: `SaveType::Web`

article fetch は `Site.fetch_article` 経由で site-owned article API adapter に委譲する。

## 補足

tag と reaction/comment count は有用な discovery metadata なので、candidate metadata として保持する。
