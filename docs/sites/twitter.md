# X / Twitter

## サイトの識別情報

- サイト名: `twitter`
- 別名: `x`, `x-twitter`
- 対応記事 URL:
  - `https://x.com/<user>/status/<id>`
  - `https://twitter.com/<user>/status/<id>`

## URL 構造

tweet URL は `x.com` または `twitter.com` 配下の `/status/<id>` を使う。

## discovery endpoint の構造

- 種類: なし

この site は現在、直接 URL fetch のみ対応する。`all` discovery には含めない。

## article fetch の方法

- fetch route: `FetchRoute::SocialStatus`
- save type: `SaveType::X`

fetch は URL から status ID を抽出し、social status 向け metadata または fallback item を返す。

## 補足

将来 timeline または search discovery を追加する場合は、新しい source namespace ではなく、この同じ site の `DiscoveryEndpoint` として追加する。
