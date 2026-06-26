# CISA KEV

## サイトの識別情報

- サイト名: `cisa-kev`
- 別名: `kev`, `cisa`
- 対応記事 URL: `https://nvd.nist.gov/vuln/detail/<CVE>`

## URL 構造

CISA KEV 自体は JSON catalog である。candidate URL は NVD CVE detail page を指す。これは後段の fetch で扱いやすい、安定した article-like page だからである。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::CatalogApi`
- request: `CatalogRequest::VulnerabilityCatalog`
- endpoint: `https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json`

discovery は KEV catalog を読み、CVE ごとに NVD detail URL を持つ candidate を 1 件生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

生成した NVD URL は `FetchRoute::GenericWeb` で取得する。

## 補足

この endpoint は article feed ではなく vulnerability discovery である。catalog と candidate URL policy を所有するため、site entry として保持する。
