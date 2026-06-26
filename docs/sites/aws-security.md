# AWS Security Bulletins

## サイトの識別情報

- サイト名: `aws-security`
- 別名: `aws-security-bulletins`, `aws-sec`
- 対応記事 URL: `https://aws.amazon.com/security/security-bulletins/<slug>/`

## URL 構造

security bulletin は `/security/security-bulletins/` 配下にある。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://aws.amazon.com/security/security-bulletins/rss/feed/`

discovery は RSS item を parse し、security bulletin candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

AWS bulletin page は `FetchRoute::GenericWeb` で取得する。

## 補足

operator が product update と security bulletin を個別に有効化・無効化できるように、`aws-whatsnew` とは分けて扱う。
