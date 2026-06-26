use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const GITHUB_SEARCH_API_URL: &str = "https://api.github.com/search/repositories";
pub const DEFAULT_GITHUB_SEARCH_QUERY: &str = "stars:>1000 pushed:>2026-01-01 archived:false";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["github.com/"])];

pub const SITE: Site = Site {
    name: "github-search",
    aliases: &["github-repos", "oss-trends"],
    supported_urls: &["https://github.com/<owner>/<repo>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: GITHUB_SEARCH_API_URL,
        default_query: Some(DEFAULT_GITHUB_SEARCH_QUERY),
        request: SearchRequest::QueryParam { name: "q" },
    }),
    parse_discovery: None,
    fetch_article: None,
};
