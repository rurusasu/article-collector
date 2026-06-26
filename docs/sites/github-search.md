# GitHub Repository Search

## サイトの識別情報

- サイト名: `github-search`
- 別名: `github-repos`, `oss-trends`
- 対応記事 URL: `https://github.com/<owner>/<repo>`

## URL 構造

repository page は `github.com/<owner>/<repo>` を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::QueryParam { name: "q" }`
- endpoint: `https://api.github.com/search/repositories`
- default query: `stars:>1000 pushed:>2026-01-01 archived:false`

discovery は GitHub Search に query し、repository candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

repository page は `FetchRoute::GenericWeb` で取得する。

## 補足

query override に対応する。この endpoint は rate limit の影響を受けやすいため、低めの default limit を維持する。
