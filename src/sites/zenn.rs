use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const ZENN_FEED_URL: &str = "https://zenn.dev/feed";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["zenn.dev/"])];

pub const SITE: Site = Site {
    name: "zenn",
    aliases: &["zenn.dev"],
    supported_urls: &["https://zenn.dev/<user>/articles/<slug>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::RssFeed {
        feed_url: ZENN_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
