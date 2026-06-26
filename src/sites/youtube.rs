use super::types::{FetchRoute, SaveType, Site, UrlRule};

const ARTICLE_RULES: &[UrlRule] = &[
    UrlRule::new(&["youtube.com/watch"]),
    UrlRule::new(&["youtu.be/"]),
];
const SAVE_RULES: &[UrlRule] = &[
    UrlRule::new(&["youtube.com/"]),
    UrlRule::new(&["youtu.be/"]),
];

pub const SITE: Site = Site {
    name: "youtube",
    aliases: &["yt"],
    supported_urls: &[
        "https://www.youtube.com/watch?v=<id>",
        "https://youtu.be/<id>",
    ],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::VideoTranscript,
    save_type: SaveType::YouTube,
    save_rules: SAVE_RULES,
    discovery: None,
    parse_discovery: None,
    fetch_article: None,
};
