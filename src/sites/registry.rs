use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, Site};
use super::{
    arxiv, aws_security, aws_whatsnew, bluesky, cisa_kev, cncf, devto, doi, github_advisory,
    github_search, google_cloud_blog, hackernews, infoq, kubernetes, martinfowler, nvd, openreview,
    qiita, thoughtworks_radar, twitter, youtube, zenn,
};

pub const SITES: &[Site] = &[
    hackernews::SITE,
    devto::SITE,
    zenn::SITE,
    twitter::SITE,
    qiita::SITE,
    bluesky::SITE,
    youtube::SITE,
    arxiv::SITE,
    github_advisory::SITE,
    cisa_kev::SITE,
    nvd::SITE,
    aws_whatsnew::SITE,
    aws_security::SITE,
    google_cloud_blog::SITE,
    kubernetes::SITE,
    cncf::SITE,
    infoq::SITE,
    martinfowler::SITE,
    github_search::SITE,
    thoughtworks_radar::SITE,
    doi::SITE,
    openreview::SITE,
];

pub fn site_by_name(name: &str) -> Option<&'static Site> {
    let normalized = normalize_name(name);
    SITES.iter().find(|site| {
        site.name == normalized || site.aliases.iter().any(|alias| *alias == normalized)
    })
}

pub fn site_for_url(url: &str) -> Option<&'static Site> {
    SITES
        .iter()
        .find(|site| site.article_rules.iter().any(|rule| rule.matches(url)))
}

pub fn fetch_route_for_url(url: &str) -> FetchRoute {
    site_for_url(url)
        .map(|site| site.fetch_route)
        .unwrap_or(FetchRoute::GenericWeb)
}

pub fn save_type_for_url(url: &str) -> SaveType {
    SITES
        .iter()
        .find(|site| site.save_rules.iter().any(|rule| rule.matches(url)))
        .map(|site| site.save_type)
        .unwrap_or(SaveType::Web)
}

pub fn discovery_endpoint_for_url(url: &str) -> Option<&'static DiscoveryEndpoint> {
    site_for_url(url)
        .filter(|site| site.name == hackernews::SITE.name)
        .and_then(|site| site.discovery.as_ref())
}

pub fn recommendable_site_names() -> Vec<&'static str> {
    recommendable_sites()
        .into_iter()
        .map(|site| site.name)
        .collect()
}

pub fn recommendable_sites() -> Vec<&'static Site> {
    SITES
        .iter()
        .filter(|site| site.discovery.is_some())
        .collect()
}

pub fn supported_url_examples() -> Vec<&'static str> {
    SITES
        .iter()
        .flat_map(|site| site.supported_urls.iter().copied())
        .collect()
}

fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}
