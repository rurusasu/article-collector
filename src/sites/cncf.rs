use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const CNCF_FEED_URL: &str = "https://www.cncf.io/feed/";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["cncf.io/"])];

pub const SITE: Site = Site {
    name: "cncf",
    aliases: &["cloud-native", "cncf-blog"],
    supported_urls: &["https://www.cncf.io/blog/<yyyy>/<mm>/<dd>/<slug>/"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::RssFeed {
        feed_url: CNCF_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
