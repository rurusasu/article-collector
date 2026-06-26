use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const ARXIV_API_URL: &str = "https://export.arxiv.org/api/query";
pub const DEFAULT_ARXIV_QUERY: &str =
    "cat:cs.AI OR cat:cs.CL OR cat:cs.CV OR cat:cs.LG OR cat:stat.ML";

const SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["arxiv.org/"])];

pub const SITE: Site = Site {
    name: "arxiv",
    aliases: &["arxiv.org"],
    supported_urls: &["https://arxiv.org/abs/<id>"],
    article_rules: &[],
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Paper,
    save_rules: SAVE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: ARXIV_API_URL,
        default_query: Some(DEFAULT_ARXIV_QUERY),
        request: SearchRequest::ArxivSearch,
    }),
    parse_discovery: None,
    fetch_article: None,
};
