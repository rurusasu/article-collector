# Kubernetes Blog

## サイトの識別情報

- サイト名: `kubernetes`
- 別名: `k8s`, `kubernetes-blog`
- 対応記事 URL: `https://kubernetes.io/blog/<yyyy>/<mm>/<dd>/<slug>/`

## URL 構造

blog post は `/blog/` 配下の日付付き path を使う。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::RssFeed`
- endpoint: `https://kubernetes.io/feed.xml`

discovery は RSS item を parse し、article candidate を生成する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`

Kubernetes blog page は `FetchRoute::GenericWeb` で取得する。

## 補足

date path structure は URL matching と docs には十分安定している。ただし discovery では feed を使う。
