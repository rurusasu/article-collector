use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const X_RECENT_SEARCH_API: &str = "https://api.x.com/2/tweets/search/recent";
pub const DEFAULT_X_RECENT_SEARCH_QUERY: &str = "(AI OR Rust OR security) lang:en -is:retweet";

const ARTICLE_RULES: &[UrlRule] = &[
    UrlRule::new(&["x.com/", "/status/"]),
    UrlRule::new(&["twitter.com/", "/status/"]),
];
const SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["x.com/"]), UrlRule::new(&["twitter.com/"])];

pub const SITE: Site = Site {
    name: "twitter",
    aliases: &["x", "x-twitter"],
    supported_urls: &[
        "https://x.com/<user>/status/<id>",
        "https://twitter.com/<user>/status/<id>",
    ],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::SocialStatus,
    save_type: SaveType::X,
    save_rules: SAVE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: X_RECENT_SEARCH_API,
        default_query: Some(DEFAULT_X_RECENT_SEARCH_QUERY),
        request: SearchRequest::XRecentSearch,
    }),
    parse_discovery: None,
    fetch_article: None,
};
