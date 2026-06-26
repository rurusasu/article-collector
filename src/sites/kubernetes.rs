use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const KUBERNETES_FEED_URL: &str = "https://kubernetes.io/feed.xml";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["kubernetes.io/blog/"])];

pub const SITE: Site = Site {
    name: "kubernetes",
    aliases: &["k8s", "kubernetes-blog"],
    supported_urls: &["https://kubernetes.io/blog/<yyyy>/<mm>/<dd>/<slug>/"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::RssFeed {
        feed_url: KUBERNETES_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
