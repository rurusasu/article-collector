use anyhow::Result;
use serde_json::{json, Map, Value};
use std::fmt;
use std::future::Future;
use std::pin::Pin;

pub type DiscoveryParser = for<'a> fn(DiscoveryPayload<'a>) -> Result<Vec<ArticleCandidate>>;
pub type ArticleFetchFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + 'a>>;
pub type ArticleFetcher = for<'a> fn(&'a str) -> ArticleFetchFuture<'a>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchRoute {
    GenericWeb,
    SocialStatus,
    VideoTranscript,
    SiteArticleApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveType {
    Web,
    Paper,
    X,
    YouTube,
}

impl SaveType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Paper => "paper",
            Self::X => "x",
            Self::YouTube => "youtube",
        }
    }
}

impl fmt::Display for SaveType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
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

impl DiscoveryEndpoint {
    pub const fn supports_query(self) -> bool {
        matches!(self, Self::SearchApi { .. })
    }

    pub const fn default_limit(self) -> Option<usize> {
        match self {
            Self::JsonApi { .. } | Self::SearchApi { .. } | Self::CatalogApi { .. } => Some(10),
            Self::RssFeed { .. } | Self::AtomFeed { .. } | Self::PageLinks => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum JsonRequest {
    PlainGet,
    PaginatedPerPage,
    FollowUpIds { item_url_template: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRequest {
    QueryParam { name: &'static str },
    ArxivSearch,
    XRecentSearch,
    QiitaItems,
    BlueskySearchPosts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CatalogRequest {
    PlainJson,
    VulnerabilityCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UrlRule {
    contains: &'static [&'static str],
}

impl UrlRule {
    pub const fn new(contains: &'static [&'static str]) -> Self {
        Self { contains }
    }

    pub fn matches(&self, url: &str) -> bool {
        self.contains.iter().all(|part| url.contains(part))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Site {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub supported_urls: &'static [&'static str],
    pub article_rules: &'static [UrlRule],
    pub fetch_route: FetchRoute,
    pub save_type: SaveType,
    pub save_rules: &'static [UrlRule],
    pub discovery: Option<DiscoveryEndpoint>,
    #[allow(dead_code)]
    pub parse_discovery: Option<DiscoveryParser>,
    pub fetch_article: Option<ArticleFetcher>,
}

#[derive(Debug)]
#[allow(dead_code)]
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
        value: &'a Value,
    },
    SearchJson {
        site: &'static Site,
        query: &'a str,
        value: &'a Value,
    },
    Links {
        site: Option<&'static Site>,
        base_url: &'a str,
        html: &'a str,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArticleCandidate {
    pub site: &'static str,
    pub title: String,
    pub url: String,
    pub rank: Option<usize>,
    pub content_hint: Option<String>,
    pub metadata: Map<String, Value>,
}

impl ArticleCandidate {
    pub fn into_value(self) -> Value {
        let mut object = self.metadata;
        object.insert("site".to_string(), json!(self.site));
        object.insert("title".to_string(), json!(self.title));
        object.insert("url".to_string(), json!(self.url));
        if let Some(rank) = self.rank {
            object.insert("rank".to_string(), json!(rank));
        }
        if let Some(content_hint) = self.content_hint {
            object.insert("content".to_string(), json!(content_hint));
        }
        Value::Object(object)
    }
}
