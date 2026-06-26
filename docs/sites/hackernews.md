# Hacker News

## サイトの識別情報

- サイト名: `hackernews`
- 別名: `hn`, `hacker-news`
- 対応記事 URL: `https://news.ycombinator.com/item?id=<id>`

## URL 構造

Hacker News の item page は `item?id=<id>` query parameter を使う。item page は discussion metadata であり、linked story は通常外部の記事 URL を指す。

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::JsonApi`
- request: `JsonRequest::FollowUpIds { item_url_template }`
- endpoint: `https://hacker-news.firebaseio.com/v0/topstories.json`
- follow-up item endpoint: `https://hacker-news.firebaseio.com/v0/item/<id>.json`

discovery は top story ID を読み、item JSON を取得し、外部 `url` が空でない item を残す。

## article fetch の方法

- fetch route: `FetchRoute::SiteArticleApi`
- save type: `SaveType::Web`

Hacker News item URL は `Site.fetch_article` 経由で site-owned article API adapter に委譲する。top stories から見つかった外部 story URL は、その URL 自身の分類に従って後段で fetch する。

## 補足

HN item には外部 URL を持たないものが多いため、discovery scan では ID を多めに取得する。
