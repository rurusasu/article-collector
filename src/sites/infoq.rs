use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const INFOQ_FEED_URL: &str = "https://feed.infoq.com/";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["infoq.com/"])];

pub const SITE: Site = Site {
    name: "infoq",
    aliases: &["infoq-news", "architecture-news"],
    supported_urls: &["https://www.infoq.com/<path>/"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::RssFeed {
        feed_url: INFOQ_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
