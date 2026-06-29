use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const BLUESKY_SEARCH_POSTS_API: &str = "https://api.bsky.app/xrpc/app.bsky.feed.searchPosts";
pub const DEFAULT_BLUESKY_QUERY: &str = "AI OR Rust OR security";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["bsky.app/profile/", "/post/"])];

pub const SITE: Site = Site {
    name: "bluesky",
    aliases: &["bsky", "bsky.app"],
    supported_urls: &["https://bsky.app/profile/<handle>/post/<rkey>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: BLUESKY_SEARCH_POSTS_API,
        default_query: Some(DEFAULT_BLUESKY_QUERY),
        request: SearchRequest::BlueskySearchPosts,
    }),
    parse_discovery: None,
    fetch_article: None,
};
