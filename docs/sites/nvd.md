# NVD

## サイトの識別情報

- サイト名: `nvd`
- 別名: `cve`, `nvd-cve`
- 対応記事 URL: `https://nvd.nist.gov/vuln/detail/<CVE>`

## URL 構造

CVE detail page は `/vuln/detail/<CVE>` を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::QueryParam { name: "keywordSearch" }`
- endpoint: `https://services.nvd.nist.gov/rest/json/cves/2.0`

discovery は NVD CVE API に query する。keyword search 用に query override に対応する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

CVE detail page は `FetchRoute::GenericWeb` で取得する。

## 補足

この endpoint は rate limit の影響を受けやすいため、低めの default limit を維持する。query support は command orchestration ではなく endpoint の責務である。
