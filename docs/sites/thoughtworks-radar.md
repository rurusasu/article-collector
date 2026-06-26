# Thoughtworks Technology Radar

## サイトの識別情報

- サイト名: `thoughtworks-radar`
- 別名: `technology-radar`, `tw-radar`
- 対応記事 URL: `https://www.thoughtworks.com/radar`

## URL 構造

public radar page は `/radar` 配下にある。

## discovery endpoint の構造

- 種類: なし

現在、安定した machine-readable feed/API は登録していない。この site は `all` discovery には含めない。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

radar page は `FetchRoute::GenericWeb` で取得する。

## 補足

安定した radar feed または dataset を追加する場合は、この site entry の discovery endpoint として扱う。
