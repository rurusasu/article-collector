use anyhow::{bail, Result};
use serde_json::Value;

use crate::sites;

pub async fn fetch_items(url: &str) -> Result<Vec<Value>> {
    let Some(site) = sites::site_for_url(url) else {
        bail!("No site article API adapter registered for URL: {url}");
    };
    let Some(fetch_article) = site.fetch_article else {
        bail!(
            "Site '{}' does not expose an article API adapter",
            site.name
        );
    };

    fetch_article(url).await
}
