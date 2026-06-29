# Qiita And Bluesky Recommend Sources Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Qiita and Bluesky as first-class recommendation sources for `article-collector recommend <site>` and configured `recommend all` runs.

**Architecture:** Add `qiita` and `bluesky` site registry modules, then dispatch source-specific search collectors from `src/recommend.rs`. Qiita uses the public Qiita items API; Bluesky uses public AppView post search. Both normalize remote responses into the existing recommendation item JSON shape and use generic web fetch for direct URLs.

**Tech Stack:** Rust stable, reqwest, serde_json, existing `SearchApi` discovery model, existing TOML recommend config, existing unit and async collector tests.

---

## File Structure

- Create `src/sites/qiita.rs`: Qiita site metadata, aliases, URL rules, default query, and discovery endpoint.
- Create `src/sites/bluesky.rs`: Bluesky site metadata, aliases, URL rules, default query, and discovery endpoint.
- Modify `src/sites/types.rs`: add `SearchRequest::QiitaItems` and `SearchRequest::BlueskySearchPosts`.
- Modify `src/sites/mod.rs`: expose new modules and registry tests.
- Modify `src/sites/registry.rs`: register Qiita and Bluesky after `twitter` and before `youtube` / `arxiv`.
- Modify `src/recommend.rs`: add Qiita and Bluesky URL builders, response normalizers, async collectors, dispatch arms, and tests.
- Modify `article-collector.toml`: add both sources and per-source default config.
- Create `docs/sites/qiita.md`: document Qiita discovery and direct fetch behavior.
- Create `docs/sites/bluesky.md`: document Bluesky discovery and direct fetch behavior.
- Modify `docs/sites/README.md`: list both site docs.
- Modify `docs/AGENTS.md`: list both source docs in the site index.
- Modify `README.md`: document commands, config, supported URL table, query source list, and `recommend all` source list.

## Task 0: Plane Issue

**Files:**
- No repository files.
- Plane project: `ACS`

- [ ] **Step 1: Create or update Plane issue before implementation**

Run:

```powershell
docker exec plane-app-api-1 python manage.py shell
```

In the Django shell, create or update the ACS issue with this content:

```python
from plane.db.models import Issue, Project, State

project = Project.objects.get(identifier="ACS")
state = State.objects.filter(project=project).exclude(group__in=["completed", "cancelled"]).first()
issue, created = Issue.objects.get_or_create(
    project=project,
    name="Add Qiita and Bluesky recommend sources",
    defaults={
        "description_html": """
<h2>Purpose</h2>
<p>Add Qiita and Bluesky as recommend sources for site-name and recommend-all runs.</p>
<h2>Implementation scope</h2>
<ul>
<li>Register qiita and bluesky site metadata.</li>
<li>Add Qiita items and Bluesky searchPosts collectors.</li>
<li>Normalize records into existing recommendation JSON.</li>
<li>Add sample config and docs.</li>
</ul>
<h2>Acceptance criteria</h2>
<ul>
<li>recommendable_site_names includes qiita and bluesky in registry order.</li>
<li>source-specific query and limit config works for both sources.</li>
<li>collector tests cover request construction, response normalization, and HTTP errors.</li>
<li>README, docs/sites, docs/AGENTS, and article-collector.toml match implementation.</li>
</ul>
<h2>Verification commands</h2>
<pre><code>cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked</code></pre>
<h2>Release and cleanup</h2>
<p>No secrets or migrations. Release notes should mention the two new public recommend sources. No branch cleanup until after PR/merge.</p>
""",
        "state": state,
    },
)
if not created:
    issue.description_html = issue.description_html + """
<h2>Latest implementation plan</h2>
<p>Use docs/superpowers/plans/2026-06-29-qiita-bluesky-recommend-sources.md.</p>
"""
    issue.save()
print(issue.sequence_id, created, issue.state.name if issue.state else None)
```

Expected: prints the ACS issue sequence ID and an active state.

- [ ] **Step 2: Record work units**

Create sub-issues or checklist-equivalent entries for:

```text
1. Registry and source planning tests
2. Qiita collector and tests
3. Bluesky collector and tests
4. Docs and sample config
5. Verification and Plane status update
```

Expected: Plane has visible work units before code editing starts.

## Task 1: Registry And Source Planning

**Files:**
- Create: `src/sites/qiita.rs`
- Create: `src/sites/bluesky.rs`
- Modify: `src/sites/types.rs`
- Modify: `src/sites/mod.rs`
- Modify: `src/sites/registry.rs`
- Modify: `src/recommend.rs`

- [ ] **Step 1: Write failing registry tests**

Add tests in `src/sites/mod.rs`:

```rust
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
```

Update `lists_recommendable_site_names()` to expect:

```rust
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
    "github-search",
]
```

Update `lists_recommendable_sites()` to expect length `18`, `sites[4].name == "qiita"`, `sites[5].name == "bluesky"`, and `sites[17].name == "github-search"`.

- [ ] **Step 2: Write failing source target and config tests**

Add tests in `src/recommend.rs`:

```rust
#[test]
fn resolves_qiita_site_name_to_items_search() {
    let RecommendationTarget::Source {
        site_name,
        endpoint:
            DiscoveryEndpoint::SearchApi {
                api_url,
                default_query,
                request: SearchRequest::QiitaItems,
            },
    } = resolve_recommendation_target("qiita").unwrap()
    else {
        panic!("qiita should resolve to Qiita items search");
    };
    assert_eq!(site_name, "qiita");
    assert_eq!(api_url, "https://qiita.com/api/v2/items");
    assert_eq!(default_query, Some("AI OR Rust OR security"));
}

#[test]
fn resolves_bluesky_site_name_to_search_posts() {
    let RecommendationTarget::Source {
        site_name,
        endpoint:
            DiscoveryEndpoint::SearchApi {
                api_url,
                default_query,
                request: SearchRequest::BlueskySearchPosts,
            },
    } = resolve_recommendation_target("bluesky").unwrap()
    else {
        panic!("bluesky should resolve to Bluesky searchPosts");
    };
    assert_eq!(site_name, "bluesky");
    assert_eq!(
        api_url,
        "https://public.api.bsky.app/xrpc/app.bsky.feed.searchPosts"
    );
    assert_eq!(default_query, Some("AI OR Rust OR security"));
}

#[test]
fn qiita_and_bluesky_source_plans_use_config_query_and_limit() {
    let config = RecommendConfig {
        sources: Some(vec!["qiita".to_string(), "bluesky".to_string()]),
        source: BTreeMap::from([
            (
                "qiita".to_string(),
                RecommendSiteConfig {
                    limit: Some(6),
                    query: Some("Rust tag:rust".to_string()),
                    ..Default::default()
                },
            ),
            (
                "bluesky".to_string(),
                RecommendSiteConfig {
                    limit: Some(8),
                    query: Some("atproto rust".to_string()),
                    ..Default::default()
                },
            ),
        ]),
        ..Default::default()
    };

    let plans = source_plans_for_all(None, &config).unwrap();

    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].site_name, "qiita");
    assert_eq!(plans[0].limit, 6);
    assert_eq!(plans[0].query.as_deref(), Some("Rust tag:rust"));
    assert_eq!(plans[1].site_name, "bluesky");
    assert_eq!(plans[1].limit, 8);
    assert_eq!(plans[1].query.as_deref(), Some("atproto rust"));
}
```

- [ ] **Step 3: Run registry/source tests to verify RED**

Run:

```powershell
cargo test --locked qiita
cargo test --locked bluesky
```

Expected: FAIL because `qiita`, `bluesky`, `SearchRequest::QiitaItems`, and `SearchRequest::BlueskySearchPosts` do not exist.

- [ ] **Step 4: Implement minimal registry support**

In `src/sites/types.rs`, change `SearchRequest` to:

```rust
pub enum SearchRequest {
    QueryParam { name: &'static str },
    ArxivSearch,
    XRecentSearch,
    QiitaItems,
    BlueskySearchPosts,
}
```

Create `src/sites/qiita.rs`:

```rust
use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const QIITA_ITEMS_API: &str = "https://qiita.com/api/v2/items";
pub const DEFAULT_QIITA_QUERY: &str = "AI OR Rust OR security";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["qiita.com/"])];

pub const SITE: Site = Site {
    name: "qiita",
    aliases: &["qiita.com"],
    supported_urls: &["https://qiita.com/<user>/items/<id>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: QIITA_ITEMS_API,
        default_query: Some(DEFAULT_QIITA_QUERY),
        request: SearchRequest::QiitaItems,
    }),
    parse_discovery: None,
    fetch_article: None,
};
```

Create `src/sites/bluesky.rs`:

```rust
use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const BLUESKY_SEARCH_POSTS_API: &str =
    "https://public.api.bsky.app/xrpc/app.bsky.feed.searchPosts";
pub const DEFAULT_BLUESKY_QUERY: &str = "AI OR Rust OR security";

const ARTICLE_RULES: &[UrlRule] = &[UrlRule::new(&["bsky.app/profile/", "/post/"])];

pub const SITE: Site = Site {
    name: "bluesky",
    aliases: &["bsky", "bsky.app"],
    supported_urls: &["https://bsky.app/profile/<handle>/post/<rkey>"],
    article_rules: ARTICLE_RULES,
    fetch_route: FetchRoute::GenericWeb,
    save_type: SaveType::Web,
    save_rules: ARTICLE_RULES,
    discovery: Some(DiscoveryEndpoint::SearchApi {
        api_url: BLUESKY_SEARCH_POSTS_API,
        default_query: Some(DEFAULT_BLUESKY_QUERY),
        request: SearchRequest::BlueskySearchPosts,
    }),
    parse_discovery: None,
    fetch_article: None,
};
```

In `src/sites/mod.rs`, add:

```rust
pub mod bluesky;
pub mod qiita;
```

In `src/sites/registry.rs`, import and register both:

```rust
use super::{
    arxiv, aws_security, aws_whatsnew, bluesky, cisa_kev, cncf, devto, doi, github_advisory,
    github_search, google_cloud_blog, hackernews, infoq, kubernetes, martinfowler, nvd,
    openreview, qiita, thoughtworks_radar, twitter, youtube, zenn,
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
    // keep the remaining sites in their existing order
];
```

- [ ] **Step 5: Run registry/source tests to verify GREEN**

Run:

```powershell
cargo test --locked qiita
cargo test --locked bluesky
```

Expected: PASS.

## Task 2: Qiita Collector

**Files:**
- Modify: `src/recommend.rs`

- [ ] **Step 1: Write failing Qiita helper and normalization tests**

Add tests in `src/recommend.rs`:

```rust
#[test]
fn builds_qiita_items_url_with_query_and_limit() {
    let url = build_qiita_items_url("https://qiita.com/api/v2/items", "Rust tag:rust", 3)
        .unwrap();

    assert_eq!(url.host_str(), Some("qiita.com"));
    assert!(url.as_str().contains("page=1"));
    assert!(url.as_str().contains("per_page=3"));
    assert!(url.as_str().contains("query=Rust+tag%3Arust"));
}

#[test]
fn normalizes_qiita_items_response() {
    let response = json!([
        {
            "title": "Rust async testing",
            "url": "https://qiita.com/alice/items/abcdef",
            "body": "# Rust async testing",
            "rendered_body": "<h1>Rust async testing</h1>",
            "likes_count": 42,
            "created_at": "2026-06-29T00:00:00+09:00",
            "updated_at": "2026-06-29T01:00:00+09:00",
            "tags": [{"name": "Rust"}, {"name": "Tokio"}],
            "user": {"id": "alice", "name": "Alice Example"}
        }
    ]);

    let items = parse_qiita_items_response(&response, 10).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source"], "qiita");
    assert_eq!(items[0]["site"], "qiita");
    assert_eq!(items[0]["title"], "Rust async testing");
    assert_eq!(items[0]["url"], "https://qiita.com/alice/items/abcdef");
    assert_eq!(items[0]["author"], "Alice Example");
    assert_eq!(items[0]["username"], "alice");
    assert_eq!(items[0]["tags"][0], "Rust");
    assert_eq!(items[0]["tags"][1], "Tokio");
    assert_eq!(items[0]["likes_count"], 42);
}
```

- [ ] **Step 2: Run Qiita helper tests to verify RED**

Run:

```powershell
cargo test --locked qiita
```

Expected: FAIL because Qiita helpers are missing.

- [ ] **Step 3: Implement Qiita URL builder and parser**

Add near source-specific helpers in `src/recommend.rs`:

```rust
fn build_qiita_items_url(api_url: &str, query: &str, limit: usize) -> Result<Url> {
    let mut url = Url::parse(api_url)?;
    url.query_pairs_mut()
        .append_pair("page", "1")
        .append_pair("per_page", &limit.to_string())
        .append_pair("query", query);
    Ok(url)
}

fn parse_qiita_items_response(response: &Value, limit: usize) -> Result<Vec<Value>> {
    let items = response
        .as_array()
        .context("Qiita items response should be a JSON array")?;
    Ok(items
        .iter()
        .take(limit)
        .enumerate()
        .filter_map(|(index, item)| qiita_item_to_recommendation(item, index + 1))
        .collect())
}

fn qiita_item_to_recommendation(item: &Value, rank: usize) -> Option<Value> {
    let url = item.get("url").and_then(Value::as_str)?;
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("Untitled");
    let content = item
        .get("body")
        .or_else(|| item.get("rendered_body"))
        .and_then(Value::as_str)
        .unwrap_or(title);
    let user = item.get("user");
    let username = user
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let author = user
        .and_then(|user| user.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(username);

    let mut recommendation = json!({
        "source": "qiita",
        "site": "qiita",
        "rank": rank,
        "title": title,
        "url": url,
        "content": content,
        "tags": qiita_tags(item)
    });

    if let Some(object) = recommendation.as_object_mut() {
        if !author.is_empty() {
            object.insert("author".to_string(), json!(author));
        }
        if !username.is_empty() {
            object.insert("username".to_string(), json!(username));
        }
        if let Some(likes) = item.get("likes_count").and_then(Value::as_u64) {
            object.insert("likes_count".to_string(), json!(likes));
        }
        if let Some(created_at) = item.get("created_at").and_then(Value::as_str) {
            object.insert("created_at".to_string(), json!(created_at));
        }
        if let Some(updated_at) = item.get("updated_at").and_then(Value::as_str) {
            object.insert("updated_at".to_string(), json!(updated_at));
        }
    }
    Some(recommendation)
}

fn qiita_tags(item: &Value) -> Vec<String> {
    item.get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.get("name").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}
```

- [ ] **Step 4: Run Qiita helper tests to verify GREEN**

Run:

```powershell
cargo test --locked qiita
```

Expected: PASS.

- [ ] **Step 5: Write failing Qiita async collector tests**

Add tests in `src/recommend.rs`:

```rust
#[tokio::test]
async fn collect_qiita_items_sends_query_and_limit() {
    let api_url = serve_qiita_items_api(
        200,
        r#"[{"title":"Rust example","url":"https://qiita.com/a/items/1","body":"body"}]"#,
    )
    .await;

    let items = collect_qiita_items(&api_url, "Rust tag:rust", 3).await.unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["site"], "qiita");
    assert_eq!(items[0]["url"], "https://qiita.com/a/items/1");
}

#[tokio::test]
async fn collect_qiita_items_reports_http_errors() {
    let api_url = serve_qiita_items_api(429, r#"{"message":"rate limited"}"#).await;

    let error = collect_qiita_items(&api_url, "Rust", 3).await.unwrap_err();

    assert!(error.to_string().contains("Qiita items search failed"));
    assert!(error.to_string().contains("429"));
}
```

Add helper:

```rust
async fn serve_qiita_items_api(status: u16, body: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request_buffer = [0_u8; 4096];
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let bytes_read = socket.read(&mut request_buffer).await.unwrap();
        let request = String::from_utf8_lossy(&request_buffer[..bytes_read]).to_string();
        assert!(request.starts_with("GET /?"));
        assert!(request.contains("page=1"));
        assert!(request.contains("per_page=3"));
        assert!(request.contains("query=Rust"));
        assert!(request.to_ascii_lowercase().contains("user-agent: article-collector/"));
        let reason = if status == 200 { "OK" } else { "Too Many Requests" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    format!("http://{address}")
}
```

- [ ] **Step 6: Run Qiita async tests to verify RED**

Run:

```powershell
cargo test --locked qiita
```

Expected: FAIL because `collect_qiita_items` is missing.

- [ ] **Step 7: Implement Qiita async collector and dispatch**

Add:

```rust
async fn collect_qiita_items(api_url: &str, query: &str, limit: usize) -> Result<Vec<Value>> {
    let url = build_qiita_items_url(api_url, query, limit)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch Qiita items from {api_url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!(
            "Qiita items search failed with HTTP status {status}. Response: {}",
            body.chars().take(500).collect::<String>()
        );
    }
    let value: Value = response
        .json()
        .await
        .context("Failed to parse Qiita items response")?;
    parse_qiita_items_response(&value, limit)
}
```

In `collect_source`, add before the generic `SearchApi` fallback:

```rust
DiscoveryEndpoint::SearchApi {
    api_url,
    default_query,
    request: SearchRequest::QiitaItems,
} => {
    collect_qiita_items(
        api_url,
        query_override_or_default(query, required_default_query(site_name, default_query)?),
        limit,
    )
    .await?
}
```

- [ ] **Step 8: Run Qiita async tests to verify GREEN**

Run:

```powershell
cargo test --locked qiita
```

Expected: PASS.

## Task 3: Bluesky Collector

**Files:**
- Modify: `src/recommend.rs`

- [ ] **Step 1: Write failing Bluesky helper and normalization tests**

Add tests in `src/recommend.rs`:

```rust
#[test]
fn builds_bluesky_search_posts_url_with_query_and_limit() {
    let url = build_bluesky_search_posts_url(
        "https://public.api.bsky.app/xrpc/app.bsky.feed.searchPosts",
        "atproto rust",
        4,
    )
    .unwrap();

    assert_eq!(url.host_str(), Some("public.api.bsky.app"));
    assert!(url.as_str().contains("q=atproto+rust"));
    assert!(url.as_str().contains("limit=4"));
    assert!(url.as_str().contains("sort=latest"));
}

#[test]
fn normalizes_bluesky_search_posts_response() {
    let response = json!({
        "posts": [{
            "uri": "at://did:plc:abc/app.bsky.feed.post/3lxyz",
            "author": {
                "did": "did:plc:abc",
                "handle": "alice.bsky.social",
                "displayName": "Alice Example"
            },
            "record": {
                "text": "Rust and AT Protocol notes\nSecond line",
                "createdAt": "2026-06-29T00:00:00.000Z"
            },
            "likeCount": 12,
            "repostCount": 3,
            "replyCount": 2
        }]
    });

    let items = parse_bluesky_search_posts_response(&response, 10).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source"], "bluesky");
    assert_eq!(items[0]["site"], "bluesky");
    assert_eq!(items[0]["title"], "Rust and AT Protocol notes");
    assert_eq!(
        items[0]["url"],
        "https://bsky.app/profile/alice.bsky.social/post/3lxyz"
    );
    assert_eq!(items[0]["author"], "Alice Example");
    assert_eq!(items[0]["handle"], "alice.bsky.social");
    assert_eq!(items[0]["did"], "did:plc:abc");
    assert_eq!(items[0]["like_count"], 12);
}
```

- [ ] **Step 2: Run Bluesky helper tests to verify RED**

Run:

```powershell
cargo test --locked bluesky
```

Expected: FAIL because Bluesky helpers are missing.

- [ ] **Step 3: Implement Bluesky URL builder and parser**

Add:

```rust
fn build_bluesky_search_posts_url(api_url: &str, query: &str, limit: usize) -> Result<Url> {
    let mut url = Url::parse(api_url)?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", &limit.to_string())
        .append_pair("sort", "latest");
    Ok(url)
}

fn parse_bluesky_search_posts_response(response: &Value, limit: usize) -> Result<Vec<Value>> {
    let posts = response
        .get("posts")
        .and_then(Value::as_array)
        .context("Bluesky searchPosts response should contain a posts array")?;
    Ok(posts
        .iter()
        .take(limit)
        .enumerate()
        .filter_map(|(index, post)| bluesky_post_to_recommendation(post, index + 1))
        .collect())
}

fn bluesky_post_to_recommendation(post: &Value, rank: usize) -> Option<Value> {
    let uri = post.get("uri").and_then(Value::as_str)?;
    let rkey = bluesky_rkey_from_uri(uri)?;
    let author = post.get("author")?;
    let handle = author.get("handle").and_then(Value::as_str)?;
    let did = author.get("did").and_then(Value::as_str).unwrap_or("");
    let display_name = author
        .get("displayName")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(handle);
    let text = post
        .get("record")
        .and_then(|record| record.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let created_at = post
        .get("record")
        .and_then(|record| record.get("createdAt"))
        .and_then(Value::as_str);
    let title = social_text_title(text, "Bluesky post", &rkey);
    let url = format!("https://bsky.app/profile/{handle}/post/{rkey}");

    let mut recommendation = json!({
        "source": "bluesky",
        "site": "bluesky",
        "rank": rank,
        "title": title,
        "url": url,
        "content": text,
        "author": display_name,
        "handle": handle
    });

    if let Some(object) = recommendation.as_object_mut() {
        if !did.is_empty() {
            object.insert("did".to_string(), json!(did));
        }
        if let Some(created_at) = created_at {
            object.insert("created_at".to_string(), json!(created_at));
        }
        copy_u64_field(post, object, "likeCount", "like_count");
        copy_u64_field(post, object, "repostCount", "repost_count");
        copy_u64_field(post, object, "replyCount", "reply_count");
    }
    Some(recommendation)
}

fn bluesky_rkey_from_uri(uri: &str) -> Option<String> {
    uri.rsplit('/').next().filter(|rkey| !rkey.is_empty()).map(ToOwned::to_owned)
}

fn social_text_title(text: &str, fallback_prefix: &str, fallback_id: &str) -> String {
    let first_line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let title = first_line.chars().take(80).collect::<String>();
    if title.trim().is_empty() {
        format!("{fallback_prefix} {fallback_id}")
    } else {
        title
    }
}

fn copy_u64_field(
    source: &Value,
    object: &mut serde_json::Map<String, Value>,
    source_name: &str,
    target_name: &str,
) {
    if let Some(value) = source.get(source_name).and_then(Value::as_u64) {
        object.insert(target_name.to_string(), json!(value));
    }
}
```

Then update `x_tweet_title` to call `social_text_title(text, "X post", tweet_id)`.

- [ ] **Step 4: Run Bluesky helper tests to verify GREEN**

Run:

```powershell
cargo test --locked bluesky
```

Expected: PASS.

- [ ] **Step 5: Write failing Bluesky async collector tests**

Add:

```rust
#[tokio::test]
async fn collect_bluesky_search_posts_sends_query_limit_and_sort() {
    let api_url = serve_bluesky_search_posts_api(
        200,
        r#"{"posts":[{"uri":"at://did:plc:abc/app.bsky.feed.post/3lxyz","author":{"did":"did:plc:abc","handle":"alice.bsky.social"},"record":{"text":"ATProto example"}}]}"#,
    )
    .await;

    let items = collect_bluesky_search_posts(&api_url, "atproto rust", 4)
        .await
        .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["url"],
        "https://bsky.app/profile/alice.bsky.social/post/3lxyz"
    );
}

#[tokio::test]
async fn collect_bluesky_search_posts_reports_http_errors() {
    let api_url = serve_bluesky_search_posts_api(429, r#"{"error":"RateLimit"}"#).await;

    let error = collect_bluesky_search_posts(&api_url, "atproto", 4)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("Bluesky searchPosts failed"));
    assert!(error.to_string().contains("429"));
}
```

Add helper:

```rust
async fn serve_bluesky_search_posts_api(status: u16, body: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request_buffer = [0_u8; 4096];
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let bytes_read = socket.read(&mut request_buffer).await.unwrap();
        let request = String::from_utf8_lossy(&request_buffer[..bytes_read]).to_string();
        assert!(request.starts_with("GET /?"));
        assert!(request.contains("q=atproto"));
        assert!(request.contains("limit=4"));
        assert!(request.contains("sort=latest"));
        assert!(request.to_ascii_lowercase().contains("user-agent: article-collector/"));
        let reason = if status == 200 { "OK" } else { "Too Many Requests" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    format!("http://{address}")
}
```

- [ ] **Step 6: Run Bluesky async tests to verify RED**

Run:

```powershell
cargo test --locked bluesky
```

Expected: FAIL because `collect_bluesky_search_posts` is missing.

- [ ] **Step 7: Implement Bluesky async collector and dispatch**

Add:

```rust
async fn collect_bluesky_search_posts(
    api_url: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>> {
    let url = build_bluesky_search_posts_url(api_url, query, limit)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch Bluesky searchPosts from {api_url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!(
            "Bluesky searchPosts failed with HTTP status {status}. Response: {}",
            body.chars().take(500).collect::<String>()
        );
    }
    let value: Value = response
        .json()
        .await
        .context("Failed to parse Bluesky searchPosts response")?;
    parse_bluesky_search_posts_response(&value, limit)
}
```

In `collect_source`, add:

```rust
DiscoveryEndpoint::SearchApi {
    api_url,
    default_query,
    request: SearchRequest::BlueskySearchPosts,
} => {
    collect_bluesky_search_posts(
        api_url,
        query_override_or_default(query, required_default_query(site_name, default_query)?),
        limit,
    )
    .await?
}
```

- [ ] **Step 8: Run Bluesky async tests to verify GREEN**

Run:

```powershell
cargo test --locked bluesky
```

Expected: PASS.

## Task 4: Docs And Config

**Files:**
- Modify: `article-collector.toml`
- Create: `docs/sites/qiita.md`
- Create: `docs/sites/bluesky.md`
- Modify: `docs/sites/README.md`
- Modify: `docs/AGENTS.md`
- Modify: `README.md`

- [ ] **Step 1: Update sample config**

Add to `[recommend].sources` after `twitter`:

```toml
  "qiita",
  "bluesky",
```

Add source config:

```toml
[recommend.source.qiita]
limit = 10
query = "AI OR Rust OR security"

[recommend.source.bluesky]
limit = 10
query = "AI OR Rust OR security"
```

- [ ] **Step 2: Add site docs**

Create `docs/sites/qiita.md` with:

```markdown
# Qiita

## サイトの識別情報

- サイト名: `qiita`
- 別名: `qiita.com`
- 対応記事 URL:
  - `https://qiita.com/<user>/items/<id>`

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::QiitaItems`
- endpoint: `https://qiita.com/api/v2/items`
- query: `[recommend.source.qiita].query`

Qiita API v2 の public items endpoint を使い、`page=1`, `per_page`, `query` で記事候補を取得する。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`
```

Create `docs/sites/bluesky.md` with:

```markdown
# Bluesky

## サイトの識別情報

- サイト名: `bluesky`
- 別名: `bsky`, `bsky.app`
- 対応記事 URL:
  - `https://bsky.app/profile/<handle>/post/<rkey>`

## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- request: `SearchRequest::BlueskySearchPosts`
- endpoint: `https://public.api.bsky.app/xrpc/app.bsky.feed.searchPosts`
- query: `[recommend.source.bluesky].query`

public AppView の `app.bsky.feed.searchPosts` を使う query-based recommendation source として扱う。

## article fetch の方法

- fetch route: `FetchRoute::GenericWeb`
- save type: `SaveType::Web`
```

- [ ] **Step 3: Update docs indexes and README**

Update `docs/sites/README.md`:

```markdown
| Qiita | [qiita.md](qiita.md) |
| Bluesky | [bluesky.md](bluesky.md) |
```

Update `docs/AGENTS.md` site table with Qiita and Bluesky rows:

```markdown
| [sites/qiita.md](sites/qiita.md) | `qiita` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | Qiita API v2 items search から技術記事候補を得る。 |
| [sites/bluesky.md](sites/bluesky.md) | `bluesky` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | public AppView searchPosts から post URL を得る。 |
```

Update `README.md` in every affected source list:

```markdown
article-collector recommend qiita --query "Rust tag:rust" --limit 10
article-collector recommend bluesky --query "atproto rust" --limit 10
```

Include `qiita` and `bluesky` in query-capable source text and in the `recommend all` current target list.

- [ ] **Step 4: Run docs/config focused tests**

Run:

```powershell
cargo test --locked recommendable_site
```

Expected: PASS.

## Task 5: Verification And Plane Status

**Files:**
- Plane issue from Task 0.

- [ ] **Step 1: Run focused feature tests**

Run:

```powershell
cargo test --locked qiita
cargo test --locked bluesky
```

Expected: PASS.

- [ ] **Step 2: Run full verification**

Run:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Expected: all commands exit 0.

- [ ] **Step 3: Update Plane issue status**

Run:

```powershell
docker exec plane-app-api-1 python manage.py shell
```

In the Django shell:

```python
from plane.db.models import Issue, State

issue = Issue.objects.get(project__identifier="ACS", name="Add Qiita and Bluesky recommend sources")
done = State.objects.filter(project=issue.project, group="completed").first()
issue.state = done
issue.save()
for child in Issue.objects.filter(parent=issue):
    child.state = done
    child.save()
print(issue.sequence_id, issue.state.name)
```

Expected: parent issue and any sub-issues/checklist-equivalent items reflect completed implementation only after verification passes.

## Self-Review

- Spec coverage: registry, Qiita collector, Bluesky collector, config, docs, verification, and Plane status are covered.
- Marker scan: no unfinished markers are used in this plan.
- Type consistency: `SearchRequest::QiitaItems`, `SearchRequest::BlueskySearchPosts`, `collect_qiita_items`, `collect_bluesky_search_posts`, and parser/helper names are consistent across tasks.
