use super::types::{DiscoveryEndpoint, FetchRoute, JsonRequest, SaveType, Site, UrlRule};

pub const GITHUB_ADVISORIES_API_URL: &str = "https://api.github.com/advisories";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["github.com/advisories/"])];

pub const SITE: Site = Site {
    name: "github-advisory",
    aliases: &["github-advisories", "ghsa"],
    supported_urls: &["https://github.com/advisories/<ghsa-id>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::JsonApi {
        api_url: GITHUB_ADVISORIES_API_URL,
        request: JsonRequest::PaginatedPerPage,
    }),
    parse_discovery: None,
    fetch_article: None,
};
