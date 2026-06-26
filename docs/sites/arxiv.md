# arXiv

## サイトの識別情報

- サイト名: `arxiv`
- 別名: `arxiv.org`
- 対応記事 URL: `https://arxiv.org/abs/<id>`

## URL 構造

paper page は `/abs/<id>` を使う。API は `http://arxiv.org/abs/<id>` を返す場合があるため、`https://arxiv.org/<path>` に正規化する。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::ArxivSearch`
- endpoint: `https://export.arxiv.org/api/query`
- default query: `cat:cs.AI OR cat:cs.CL OR cat:cs.CV OR cat:cs.LG OR cat:stat.ML`

discovery は Atom query を組み立て、entry を paper candidate に parse する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Paper`

paper page は `FetchRoute::GenericWeb` で取得する。save classification は `SaveType::Paper`。

## 補足

この discovery endpoint は `--query` と `[recommend.source.arxiv].query` に対応する。
