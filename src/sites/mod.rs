pub mod arxiv;
pub mod aws_security;
pub mod aws_whatsnew;
pub mod bluesky;
pub mod cisa_kev;
pub mod cncf;
pub mod devto;
pub mod doi;
pub mod github_advisory;
pub mod github_search;
pub mod google_cloud_blog;
pub mod hackernews;
pub mod infoq;
pub mod kubernetes;
pub mod martinfowler;
pub mod nvd;
pub mod openreview;
pub mod qiita;
mod registry;
pub mod thoughtworks_radar;
pub mod twitter;
pub mod types;
pub mod youtube;
pub mod zenn;

pub use registry::{
    discovery_endpoint_for_url, fetch_route_for_url, recommendable_site_names, recommendable_sites,
    save_type_for_url, site_by_name, site_for_url, supported_url_examples, SITES,
};
pub use types::FetchRoute;

#[cfg(test)]
mod tests {
    use super::types::{DiscoveryEndpoint, JsonRequest, SaveType, SearchRequest};
    use super::*;

    #[test]
    fn resolves_site_by_name_and_alias() {
        assert_eq!(site_by_name("hackernews").unwrap().name, "hackernews");
        assert_eq!(site_by_name("HN").unwrap().name, "hackernews");
        assert_eq!(site_by_name("dev.to").unwrap().name, "devto");
    }

    #[test]
    fn resolves_github_advisory_site_name_aliases_and_discovery_endpoint() {
        let site = site_by_name("github-advisory").unwrap();
        assert_eq!(site.name, "github-advisory");
        assert_eq!(site_by_name("github-advisories").unwrap().name, site.name);
        assert_eq!(site_by_name("ghsa").unwrap().name, site.name);
        assert!(matches!(
            site.discovery,
            Some(DiscoveryEndpoint::JsonApi { .. })
        ));
        assert!(recommendable_site_names().contains(&"github-advisory"));
        assert!(supported_url_examples().contains(&"https://github.com/advisories/<ghsa-id>"));
    }

    #[test]
    fn resolves_cto_discovery_aliases() {
        let cases = [
            ("cisa-kev", &["kev", "cisa"][..]),
            ("nvd", &["cve", "nvd-cve"][..]),
            ("aws-whatsnew", &["aws-new", "aws"][..]),
            ("aws-security", &["aws-security-bulletins", "aws-sec"][..]),
            ("google-cloud-blog", &["google-cloud", "gcp"][..]),
            ("kubernetes", &["k8s", "kubernetes-blog"][..]),
            ("cncf", &["cloud-native", "cncf-blog"][..]),
            ("infoq", &["infoq-news", "architecture-news"][..]),
            ("martinfowler", &["fowler", "martin-fowler"][..]),
            ("github-search", &["github-repos", "oss-trends"][..]),
        ];

        let recommendable = recommendable_site_names();

        for (name, aliases) in cases {
            let site = site_by_name(name).unwrap();
            assert_eq!(site.name, name);
            assert!(
                recommendable.contains(&name),
                "{name} should be recommend-enabled"
            );
            for alias in aliases {
                assert_eq!(site_by_name(alias).unwrap().name, name);
            }
            assert!(site.discovery.is_some(), "{name} should have discovery");
        }
    }

    #[test]
    fn registers_thoughtworks_radar_as_manual_fallback_only() {
        let site = site_by_name("thoughtworks-radar").unwrap();
        assert_eq!(site_by_name("technology-radar").unwrap().name, site.name);
        assert_eq!(site_by_name("tw-radar").unwrap().name, site.name);
        assert!(site.discovery.is_none());
        assert!(!recommendable_site_names().contains(&"thoughtworks-radar"));
        assert!(supported_url_examples().contains(&"https://www.thoughtworks.com/radar"));
    }

    #[test]
    fn resolves_twitter_aliases_and_recent_search_discovery() {
        let site = site_by_name("twitter").unwrap();

        assert_eq!(site_by_name("x").unwrap().name, site.name);
        assert_eq!(site_by_name("x-twitter").unwrap().name, site.name);
        assert!(recommendable_site_names().contains(&"twitter"));
        assert!(matches!(
            site.discovery,
            Some(DiscoveryEndpoint::SearchApi {
                request: SearchRequest::XRecentSearch,
                ..
            })
        ));
    }

    #[test]
    fn resolves_qiita_aliases_and_items_discovery() {
        let site = site_by_name("qiita").unwrap();

        assert_eq!(site_by_name("qiita.com").unwrap().name, site.name);
        assert!(recommendable_site_names().contains(&"qiita"));
        assert!(matches!(
            site.discovery,
            Some(DiscoveryEndpoint::SearchApi {
                request: SearchRequest::QiitaItems,
                ..
            })
        ));
    }

    #[test]
    fn resolves_bluesky_aliases_and_search_posts_discovery() {
        let site = site_by_name("bluesky").unwrap();

        assert_eq!(site_by_name("bsky").unwrap().name, site.name);
        assert_eq!(site_by_name("bsky.app").unwrap().name, site.name);
        assert!(recommendable_site_names().contains(&"bluesky"));
        assert!(matches!(
            site.discovery,
            Some(DiscoveryEndpoint::SearchApi {
                request: SearchRequest::BlueskySearchPosts,
                ..
            })
        ));
    }

    #[test]
    fn site_metadata_uses_discovery_endpoint_not_recommend_source() {
        let site = site_by_name("hackernews").unwrap();

        assert!(matches!(
            site.discovery,
            Some(DiscoveryEndpoint::JsonApi {
                request: JsonRequest::FollowUpIds { .. },
                ..
            })
        ));
    }

    #[test]
    fn resolves_fetch_routes_from_url_rules() {
        assert_eq!(
            fetch_route_for_url("https://news.ycombinator.com/item?id=123"),
            FetchRoute::SiteArticleApi
        );
        assert_eq!(
            fetch_route_for_url("https://dev.to/author/slug"),
            FetchRoute::SiteArticleApi
        );
        assert_eq!(
            fetch_route_for_url("https://x.com/user/status/123"),
            FetchRoute::SocialStatus
        );
        assert_eq!(
            fetch_route_for_url("https://www.youtube.com/watch?v=abc"),
            FetchRoute::VideoTranscript
        );
        assert_eq!(
            fetch_route_for_url("https://example.com/article"),
            FetchRoute::GenericWeb
        );
    }

    #[test]
    fn resolves_save_type_from_url_rules() {
        assert_eq!(
            save_type_for_url("https://x.com/user/status/123"),
            SaveType::X
        );
        assert_eq!(save_type_for_url("https://youtu.be/abc"), SaveType::YouTube);
        assert_eq!(
            save_type_for_url("https://arxiv.org/abs/2301.12345"),
            SaveType::Paper
        );
        assert_eq!(
            save_type_for_url("https://doi.org/10.1234/example"),
            SaveType::Paper
        );
        assert_eq!(
            save_type_for_url("https://openreview.net/forum?id=abc"),
            SaveType::Paper
        );
        assert_eq!(
            save_type_for_url("https://example.com/article"),
            SaveType::Web
        );
    }

    #[test]
    fn resolves_hackernews_discovery_endpoint_from_url() {
        assert_eq!(
            discovery_endpoint_for_url("https://news.ycombinator.com/item?id=123"),
            Some(&DiscoveryEndpoint::JsonApi {
                api_url: hackernews::HN_TOPSTORIES_API,
                request: JsonRequest::FollowUpIds {
                    item_url_template: hackernews::HN_ITEM_API_TEMPLATE,
                },
            })
        );
    }

    #[test]
    fn does_not_replace_page_link_urls_with_site_default() {
        assert_eq!(
            discovery_endpoint_for_url("https://dev.to/author/slug"),
            None
        );
    }

    #[test]
    fn lists_recommendable_site_names() {
        assert_eq!(
            recommendable_site_names(),
            vec![
                "hackernews",
                "devto",
                "zenn",
                "twitter",
                "qiita",
                "bluesky",
                "arxiv",
                "github-advisory",
                "cisa-kev",
                "nvd",
                "aws-whatsnew",
                "aws-security",
                "google-cloud-blog",
                "kubernetes",
                "cncf",
                "infoq",
                "martinfowler",
                "github-search"
            ]
        );
    }

    #[test]
    fn lists_recommendable_sites() {
        let sites = recommendable_sites();
        assert_eq!(sites.len(), 18);
        assert_eq!(sites[0].name, "hackernews");
        assert_eq!(sites[1].name, "devto");
        assert_eq!(sites[2].name, "zenn");
        assert_eq!(sites[3].name, "twitter");
        assert_eq!(sites[4].name, "qiita");
        assert_eq!(sites[5].name, "bluesky");
        assert_eq!(sites[6].name, "arxiv");
        assert_eq!(sites[7].name, "github-advisory");
        assert_eq!(sites[8].name, "cisa-kev");
        assert_eq!(sites[17].name, "github-search");
        assert!(sites.iter().all(|site| site.discovery.is_some()));
    }

    #[test]
    fn lists_supported_url_examples() {
        let examples = supported_url_examples();
        assert!(examples.contains(&"https://news.ycombinator.com/item?id=<id>"));
        assert!(examples.contains(&"https://dev.to/<author>/<slug>"));
    }
}
