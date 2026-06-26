# YouTube

## サイトの識別情報

- サイト名: `youtube`
- 別名: `yt`
- 対応記事 URL:
  - `https://www.youtube.com/watch?v=<id>`
  - `https://youtu.be/<id>`

## URL 構造

YouTube video は `watch?v=<id>` query または短縮形式の `youtu.be/<id>` path で表される。

## discovery endpoint の構造

- 種類: なし

この site は現在、直接 URL fetch のみ対応する。将来の discovery endpoint として channel RSS や API-key search を追加できる。

## article fetch の方法

- fetch route: `FetchRoute::VideoTranscript`
- save type: `SaveType::YouTube`

fetch は video ID を抽出し、video transcript/content path を使う。

## 補足

video discovery と transcript fetch は分けて扱う。将来の channel feed は video candidate を生成し、同じ YouTube fetch route に流す。
