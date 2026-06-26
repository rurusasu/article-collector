use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const NVD_CVES_API_URL: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["nvd.nist.gov/vuln/detail/"])];

pub const SITE: Site = Site {
    name: "nvd",
    aliases: &["cve", "nvd-cve"],
    supported_urls: &["https://nvd.nist.gov/vuln/detail/<CVE>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: NVD_CVES_API_URL,
        default_query: None,
        request: SearchRequest::QueryParam {
            name: "keywordSearch",
        },
    }),
    parse_discovery: None,
    fetch_article: None,
};
