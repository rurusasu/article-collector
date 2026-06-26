# fetch / discovery enum 構成

この文書は、fetch / discovery の enum と data model の構成規約を定義する。実装はこの規約に従う。

## 概念 map

```text
Site
  alias、URL rule、article route、save type、optional な discovery endpoint を所有する

DiscoveryEndpoint
  記事候補 URL を列挙するための機構を表す

FetchRoute
  1 件の記事 URL を取得するための機構を表す

SaveType
  Markdown 書き出し時の取得 content 種別を分類する
```

## `Site`

```rust
pub struct Site {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub supported_urls: &'static [&'static str],
    pub article_rules: &'static [UrlRule],
    pub fetch_route: FetchRoute,
    pub save_type: SaveType,
    pub save_rules: &'static [UrlRule],
    pub discovery: Option<DiscoveryEndpoint>,
    pub parse_discovery: Option<DiscoveryParser>,
    pub fetch_article: Option<ArticleFetcher>,
}
```

`Site` が信頼できる唯一の定義になる。記事 fetch 用と discovery 用で同じサイトを別々に表現してはいけない。サイト固有の response parsing や article API fetcher が必要な場合、その adapter は site module に置く。

```rust
pub type DiscoveryParser = fn(DiscoveryPayload<'_>) -> anyhow::Result<Vec<ArticleCandidate>>;
pub type ArticleFetcher = fn(&str) -> ArticleFetchFuture;
```

具体的な async adapter の形は実装時に変えてよい。重要なのは、サイト固有 adapter を `Site` から参照し、`fetch/` や `discovery/` 配下にサイト名付きファイルとして置かないこと。

## `DiscoveryEndpoint`

```rust
pub enum DiscoveryEndpoint {
    RssFeed {
        feed_url: &'static str,
    },
    AtomFeed {
        feed_url: &'static str,
    },
    JsonApi {
        api_url: &'static str,
        request: JsonRequest,
    },
    SearchApi {
        api_url: &'static str,
        default_query: Option<&'static str>,
        request: SearchRequest,
    },
    CatalogApi {
        catalog_url: &'static str,
        request: CatalogRequest,
    },
    PageLinks,
}

pub enum JsonRequest {
    PlainGet,
    PaginatedPerPage,
    FollowUpIds { item_url_template: &'static str },
}

pub enum SearchRequest {
    QueryParam { name: &'static str },
    ArxivSearch,
}

pub enum CatalogRequest {
    PlainJson,
    VulnerabilityCatalog,
}
```

`DiscoveryEndpoint` は別の source namespace ではなく、取得機構そのものを表す。response を `ArticleCandidate` に変換する責務は site module が持つ。

例:

```rust
pub const SITE: Site = Site {
    name: "hackernews",
    discovery: Some(DiscoveryEndpoint::JsonApi {
        api_url: "https://hacker-news.firebaseio.com/v0/topstories.json",
        request: JsonRequest::FollowUpIds {
            item_url_template: "https://hacker-news.firebaseio.com/v0/item/{id}.json",
        },
    }),
    parse_discovery: Some(parse_hackernews_items),
    fetch_article: Some(fetch_hackernews_item),
    // 他の field は省略
};
```

## `DiscoveryPayload`

```rust
pub enum DiscoveryPayload<'a> {
    Rss {
        site: &'static Site,
        feed: &'a str,
    },
    Atom {
        site: &'static Site,
        feed: &'a str,
    },
    Json {
        site: &'static Site,
        value: serde_json::Value,
    },
    SearchJson {
        site: &'static Site,
        query: &'a str,
        value: serde_json::Value,
    },
    Links {
        site: Option<&'static Site>,
        base_url: &'a str,
        html: &'a str,
    },
}
```

## `FetchRoute`

```rust
pub enum FetchRoute {
    GenericWeb,
    SocialStatus,
    VideoTranscript,
    SiteArticleApi,
}
```

`FetchRoute` は 1 件の記事または content URL を取得するための概念に限定する。discovery は行わず、サイト名も encode しない。`SiteArticleApi` は `Site.fetch_article` に委譲する。

## `SaveType`

```rust
pub enum SaveType {
    Web,
    Paper,
    X,
    YouTube,
}
```

`SaveType` は保存分類を型として表現し、文字列の drift を避ける。

## `ArticleCandidate`

```rust
pub struct ArticleCandidate {
    pub site: &'static str,
    pub title: String,
    pub url: String,
    pub rank: Option<usize>,
    pub content_hint: Option<String>,
    pub metadata: serde_json::Map<String, serde_json::Value>,
}
```

discovery endpoint は candidate を返す。article fetch layer は candidate または URL を受け取り、取得した content で enrich する。別の `source` field は意図的に持たない。`site` が source identity である。

## query 対応

query 対応可否は discovery endpoint の性質として扱う。

```rust
impl DiscoveryEndpoint {
    pub fn supports_query(&self) -> bool {
        matches!(self, Self::SearchApi { .. })
    }
}
```

これにより、command orchestration に query behavior を hard-code しなくて済む。

## 既定 limit

低めの既定 limit も endpoint behavior の近くに置く。

```rust
impl DiscoveryEndpoint {
    pub fn default_limit(&self) -> Option<usize> {
        match self {
            Self::JsonApi { .. } | Self::SearchApi { .. } | Self::CatalogApi { .. } => Some(10),
            _ => None,
        }
    }
}
```

これにより rate limit の影響を受けやすい挙動を endpoint 機構の近くに置ける。site は config 経由で limit を上書きしてよい。

## CLI/config 命名規約

外部 config では site 名を key として使う。内部 code では `discovery` 系の名前を優先する。
