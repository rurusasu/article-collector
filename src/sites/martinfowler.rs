use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const MARTIN_FOWLER_FEED_URL: &str = "https://martinfowler.com/feed.atom";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["martinfowler.com/"])];

pub const SITE: Site = Site {
    name: "martinfowler",
    aliases: &["fowler", "martin-fowler"],
    supported_urls: &["https://martinfowler.com/articles/<slug>.html"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::AtomFeed {
        feed_url: MARTIN_FOWLER_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
