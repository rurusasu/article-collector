use super::types::{FetchRoute, SaveType, Site, UrlRule};

const SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["doi.org/"])];

pub const SITE: Site = Site {
    name: "doi",
    aliases: &["doi.org"],
    supported_urls: &["https://doi.org/<doi>"],
    article_rules: &[],
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Paper,
    save_rules: SAVE_RULES,
    discovery: None,
    parse_discovery: None,
    fetch_article: None,
};
