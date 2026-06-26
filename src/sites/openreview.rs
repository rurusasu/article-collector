use super::types::{FetchRoute, SaveType, Site, UrlRule};

const SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["openreview.net/"])];

pub const SITE: Site = Site {
    name: "openreview",
    aliases: &["openreview.net"],
    supported_urls: &["https://openreview.net/forum?id=<id>"],
    article_rules: &[],
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Paper,
    save_rules: SAVE_RULES,
    discovery: None,
    parse_discovery: None,
    fetch_article: None,
};
