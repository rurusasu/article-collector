use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const AWS_WHATSNEW_FEED_URL: &str = "https://aws.amazon.com/new/feed/";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["aws.amazon.com/"])];

pub const SITE: Site = Site {
    name: "aws-whatsnew",
    aliases: &["aws-new", "aws"],
    supported_urls: &["https://aws.amazon.com/about-aws/whats-new/<slug>/"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::RssFeed {
        feed_url: AWS_WHATSNEW_FEED_URL,
    }),
    parse_discovery: None,
    fetch_article: None,
};
