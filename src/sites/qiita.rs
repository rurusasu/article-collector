use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const QIITA_ITEMS_API: &str = "https://qiita.com/api/v2/items";
pub const DEFAULT_QIITA_QUERY: &str = "AI OR Rust OR security";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["qiita.com/"])];

pub const SITE: Site = Site {
    name: "qiita",
    aliases: &["qiita.com"],
    supported_urls: &["https://qiita.com/<user>/items/<id>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: QIITA_ITEMS_API,
        default_query: Some(DEFAULT_QIITA_QUERY),
        request: SearchRequest::QiitaItems,
    }),
    parse_discovery: None,
    fetch_article: None,
};
