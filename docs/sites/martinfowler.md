# Martin Fowler

## サイトの識別情報

- サイト名: `martinfowler`
- 別名: `fowler`, `martin-fowler`
- 対応記事 URL: `https://martinfowler.com/articles/<slug>.html`

## URL 構造

多くの記事は `/articles/<slug>.html` 配下にあるが、この site には bliki など他の path も存在する。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::AtomFeed`
- endpoint: `https://martinfowler.com/feed.atom`

discovery は Atom entry を parse し、candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

page は `FetchRoute::GenericWeb` で取得する。

## 補足

Atom parsing は他の Atom-based endpoint と共有する。
