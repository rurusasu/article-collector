use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const GOOGLE_CLOUD_BLOG_FEED_URL: &str = "https://cloudblog.withgoogle.com/rss";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["cloud.google.com/"])];

pub const SITE: Site = Site {
    name: "google-cloud-blog",
    aliases: &["google-cloud", "gcp"],
    supported_urls: &["https://cloud.google.com/blog/<slug>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::RssFeed {
        feed_url: GOOGLE_CLOUD_BLOG_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
