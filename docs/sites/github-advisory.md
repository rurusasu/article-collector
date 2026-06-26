# GitHub Advisory

## サイトの識別情報

- サイト名: `github-advisory`
- 別名: `github-advisories`, `ghsa`
- 対応記事 URL: `https://github.com/advisories/<ghsa-id>`

## URL 構造

advisory page は `/advisories/<ghsa-id>` 配下にある。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::JsonApi`
- request: `JsonRequest::PaginatedPerPage`
- endpoint: `https://api.github.com/advisories`

discovery は `per_page=<limit>` 付きで Global Security Advisories API を呼び出し、advisory metadata を正規化する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

advisory HTML page は `FetchRoute::GenericWeb` で取得する。

## 補足

この endpoint は rate limit の影響を受けやすいため、低めの default limit を維持する。
