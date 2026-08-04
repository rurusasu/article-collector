use super::types::{FetchRoute, SaveType, Site, UrlRule};

const ARTICLE_RULES: &[UrlRule] = &[
    UrlRule::new(&["x.com/", "/status/"]),
    UrlRule::new(&["twitter.com/", "/status/"]),
];
const SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["x.com/"]), UrlRule::new(&["twitter.com/"])];

pub const SITE: Site = Site {
    name: "twitter",
    aliases: &["x", "x-twitter"],
    supported_urls: &[
        "https://x.com/<user>/status/<id>",
        "https://twitter.com/<user>/status/<id>",
    ],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::SocialStatus,
    save_type: SaveType::X,
    save_rules: SAVE_RULES,
    discovery: None,
    parse_discovery: None,
    fetch_article: None,
};
