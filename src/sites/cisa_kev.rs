use super::types::{CatalogRequest, DiscoveryEndpoint, FetchRoute, SaveType, Site, UrlRule};

pub const CISA_KEV_CATALOG_URL: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["nvd.nist.gov/vuln/detail/"])];

pub const SITE: Site = Site {
    name: "cisa-kev",
    aliases: &["kev", "cisa"],
    supported_urls: &["https://nvd.nist.gov/vuln/detail/<CVE>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::CatalogApi {
        catalog_url: CISA_KEV_CATALOG_URL,
        request: CatalogRequest::VulnerabilityCatalog,
    }),
    parse_discovery: None,
    fetch_article: None,
};
