use super::types::{FetchRoute, SaveType, Site, UrlRule};

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["thoughtworks.com/radar"])];

pub const SITE: Site = Site {
    name: "thoughtworks-radar",
    aliases: &["technology-radar", "tw-radar"],
    supported_urls: &["https://www.thoughtworks.com/radar"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: None,
    parse_discovery: None,
    fetch_article: None,
};
