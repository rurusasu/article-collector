# AWS What's New

## サイトの識別情報

- サイト名: `aws-whatsnew`
- 別名: `aws-new`, `aws`
- 対応記事 URL: `https://aws.amazon.com/about-aws/whats-new/<slug>/`

## URL 構造

What's New post は `/about-aws/whats-new/` 配下にある。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://aws.amazon.com/new/feed/`

discovery は RSS item を parse し、article candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

AWS post page は `FetchRoute::GenericWeb` で取得する。

## 補足

AWS Security と AWS URL rule を共有するが、discovery feed と article URL family が異なるため、別の site entry として扱う。
