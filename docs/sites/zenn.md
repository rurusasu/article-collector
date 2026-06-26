# Zenn

## サイトの識別情報

- サイト名: `zenn`
- 別名: `zenn.dev`
- 対応記事 URL: `https://zenn.dev/<user>/articles/<slug>`

## URL 構造

article URL は `/articles/` 配下に user name と article slug を含む。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://zenn.dev/feed`

discovery は feed を parse し、title、URL、author、published time、存在する場合は description を持つ candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

article page は `FetchRoute::GenericWeb` で取得する。

## 補足

Zenn discovery は feed-based だが、article fetch は URL-driven のままにする。
