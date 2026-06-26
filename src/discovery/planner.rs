use super::types::DiscoveryEndpoint;
use crate::sites::{self, types::Site};

pub fn endpoint_for_site(site: &'static Site) -> Option<DiscoveryEndpoint> {
    site.discovery
}

pub fn endpoint_for_url(url: &str) -> Option<&'static DiscoveryEndpoint> {
    sites::discovery_endpoint_for_url(url)
}

pub fn recommendable_sites() -> Vec<&'static Site> {
    sites::recommendable_sites()
}
