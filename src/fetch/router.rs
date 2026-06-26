use crate::sites::{self, types::FetchRoute};

pub fn classify_url(url: &str) -> FetchRoute {
    sites::fetch_route_for_url(url)
}
