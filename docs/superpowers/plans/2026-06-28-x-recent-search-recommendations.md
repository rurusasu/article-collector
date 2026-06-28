# X Recent Search Recommendations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add X/Twitter recent-search recommendation discovery to `article-collector recommend twitter` and config-driven `recommend all` runs.

**Architecture:** Keep X/Twitter in the existing site registry and add a `SearchRequest::XRecentSearch` route for X API v2 recent search. Keep direct tweet URL fetch unchanged, and add a small source-specific collector in `src/recommend.rs` that reads a bearer token from environment, calls the X API, and normalizes tweet/search response JSON into the current recommendation item shape.

**Tech Stack:** Rust stable, reqwest, serde_json, existing TOML config, existing `recommend.rs` collector tests.

---

## File Structure

- Modify `src/sites/types.rs`: add `SearchRequest::XRecentSearch`.
- Modify `src/sites/twitter.rs`: add X recent search constants and a `DiscoveryEndpoint::SearchApi` registration.
- Modify `src/sites/mod.rs`: add registry tests for Twitter as a recommendable source.
- Modify `src/recommend.rs`: add source resolution tests, source-plan tests, X token/request/response helpers, and X collector dispatch.
- Modify `article-collector.toml`: add `twitter` to configured sources and add `[recommend.source.twitter]`.
- Modify `docs/sites/twitter.md`: document recent-search discovery and token requirements.
- Modify `README.md`: document `twitter` in recommend source behavior and environment variables.

## Task 1: Registry And Source Planning

**Files:**
- Modify: `src/sites/types.rs`
- Modify: `src/sites/twitter.rs`
- Modify: `src/sites/mod.rs`
- Modify: `src/recommend.rs`

- [ ] **Step 1: Write failing registry tests**

Add tests in `src/sites/mod.rs`:

```rust
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
```

Also update `lists_recommendable_site_names()` and `lists_recommendable_sites()` to include `twitter` after `zenn`, preserving registry order.

- [ ] **Step 2: Write failing recommend target/source-plan tests**

Add tests in `src/recommend.rs`:

```rust
#[test]
fn resolves_twitter_site_name_to_recent_search() {
    let RecommendationTarget::Source {
        site_name,
        endpoint:
            DiscoveryEndpoint::SearchApi {
                api_url,
                default_query,
                request: SearchRequest::XRecentSearch,
            },
    } = resolve_recommendation_target("twitter").unwrap()
    else {
        panic!("twitter should resolve to X recent search");
    };
    assert_eq!(site_name, "twitter");
    assert_eq!(api_url, "https://api.x.com/2/tweets/search/recent");
    assert!(default_query.unwrap().contains("-is:retweet"));
}

#[test]
fn twitter_source_plan_uses_config_query_and_limit() {
    let config = RecommendConfig {
        sources: Some(vec!["twitter".to_string()]),
        source: BTreeMap::from([(
            "twitter".to_string(),
            RecommendSiteConfig {
                limit: Some(7),
                query: Some("rust lang:en -is:retweet".to_string()),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    let plans = source_plans_for_all(None, &config).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].site_name, "twitter");
    assert_eq!(plans[0].limit, 7);
    assert_eq!(plans[0].query.as_deref(), Some("rust lang:en -is:retweet"));
}
```

- [ ] **Step 3: Run tests to verify RED**

Run:

```powershell
cargo test --locked sites::tests::resolves_twitter_aliases_and_recent_search_discovery recommend::tests::resolves_twitter_site_name_to_recent_search recommend::tests::twitter_source_plan_uses_config_query_and_limit
```

Expected: FAIL because `SearchRequest::XRecentSearch` does not exist and `twitter` has no discovery endpoint.

- [ ] **Step 4: Implement minimal registry support**

In `src/sites/types.rs`, change `SearchRequest` to:

```rust
pub enum SearchRequest {
    QueryParam { name: &'static str },
    ArxivSearch,
    XRecentSearch,
}
```

In `src/sites/twitter.rs`, replace the import and `discovery` field with:

```rust
use super::types::{DiscoveryEndpoint, FetchRoute, SaveType, SearchRequest, Site, UrlRule};

pub const X_RECENT_SEARCH_API: &str = "https://api.x.com/2/tweets/search/recent";
pub const DEFAULT_X_RECENT_SEARCH_QUERY: &str = "(AI OR Rust OR security) lang:en -is:retweet";

// ...
discovery: Some(DiscoveryEndpoint::SearchApi {
    api_url: X_RECENT_SEARCH_API,
    default_query: Some(DEFAULT_X_RECENT_SEARCH_QUERY),
    request: SearchRequest::XRecentSearch,
}),
```

Update `src/sites/mod.rs` imports to include `SearchRequest`, and update expected recommendable names/length.

- [ ] **Step 5: Run tests to verify GREEN**

Run:

```powershell
cargo test --locked sites::tests::resolves_twitter_aliases_and_recent_search_discovery recommend::tests::resolves_twitter_site_name_to_recent_search recommend::tests::twitter_source_plan_uses_config_query_and_limit
```

Expected: PASS.

## Task 2: X Recent Search Collector

**Files:**
- Modify: `src/recommend.rs`

- [ ] **Step 1: Write failing collector helper tests**

Add tests in `src/recommend.rs`:

```rust
#[test]
fn reports_missing_x_bearer_token() {
    let error = x_bearer_token_from_env(|_| None).unwrap_err();
    assert!(error.to_string().contains("X_BEARER_TOKEN"));
    assert!(error.to_string().contains("TWITTER_BEARER_TOKEN"));
}

#[test]
fn prefers_x_bearer_token_over_twitter_bearer_token() {
    let token = x_bearer_token_from_env(|name| match name {
        "X_BEARER_TOKEN" => Some("x-token".to_string()),
        "TWITTER_BEARER_TOKEN" => Some("twitter-token".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(token, "x-token");
}

#[test]
fn normalizes_x_recent_search_response() {
    let response = json!({
        "data": [{
            "id": "1800000000000000000",
            "text": "Rust async tooling keeps getting better\\nSecond line",
            "author_id": "42",
            "created_at": "2026-06-28T00:00:00.000Z",
            "public_metrics": {
                "retweet_count": 3,
                "reply_count": 2,
                "like_count": 10,
                "quote_count": 1
            }
        }],
        "includes": {
            "users": [{
                "id": "42",
                "username": "alice",
                "name": "Alice Example"
            }]
        }
    });

    let items = parse_x_recent_search_response(&response, 10).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source"], "twitter");
    assert_eq!(items[0]["site"], "twitter");
    assert_eq!(items[0]["tweet_id"], "1800000000000000000");
    assert_eq!(items[0]["title"], "Rust async tooling keeps getting better");
    assert_eq!(
        items[0]["url"],
        "https://x.com/alice/status/1800000000000000000"
    );
    assert_eq!(items[0]["author"], "Alice Example");
    assert_eq!(items[0]["username"], "alice");
    assert_eq!(items[0]["public_metrics"]["like_count"], 10);
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cargo test --locked recommend::tests::reports_missing_x_bearer_token recommend::tests::prefers_x_bearer_token_over_twitter_bearer_token recommend::tests::normalizes_x_recent_search_response
```

Expected: FAIL because the helper functions are missing.

- [ ] **Step 3: Implement helper functions**

Add helper functions in `src/recommend.rs` near other source-specific helpers:

```rust
fn x_bearer_token_from_env<F>(get_env: F) -> Result<String>
where
    F: Fn(&str) -> Option<String>,
{
    get_env("X_BEARER_TOKEN")
        .or_else(|| get_env("TWITTER_BEARER_TOKEN"))
        .filter(|token| !token.trim().is_empty())
        .context("X recent search requires X_BEARER_TOKEN or TWITTER_BEARER_TOKEN")
}

fn x_bearer_token() -> Result<String> {
    x_bearer_token_from_env(|name| std::env::var(name).ok())
}
```

Add response normalization:

```rust
fn parse_x_recent_search_response(response: &Value, limit: usize) -> Result<Vec<Value>> {
    let users = x_user_lookup(response);
    let Some(data) = response.get("data").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    Ok(data
        .iter()
        .take(limit)
        .enumerate()
        .filter_map(|(index, tweet)| normalize_x_tweet(tweet, index + 1, &users))
        .collect())
}

fn x_user_lookup(response: &Value) -> HashMap<String, Value> {
    response
        .get("includes")
        .and_then(|includes| includes.get("users"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|user| {
            user.get("id")
                .and_then(Value::as_str)
                .map(|id| (id.to_string(), user.clone()))
        })
        .collect()
}

fn normalize_x_tweet(
    tweet: &Value,
    rank: usize,
    users: &HashMap<String, Value>,
) -> Option<Value> {
    let tweet_id = tweet.get("id").and_then(Value::as_str)?;
    let text = tweet.get("text").and_then(Value::as_str).unwrap_or("");
    let author_id = tweet.get("author_id").and_then(Value::as_str);
    let user = author_id.and_then(|id| users.get(id));
    let username = user
        .and_then(|user| user.get("username"))
        .and_then(Value::as_str);
    let author = user
        .and_then(|user| user.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let url = x_tweet_url(username, tweet_id);
    let title = x_tweet_title(text, tweet_id);

    let mut item = json!({
        "source": "twitter",
        "site": "twitter",
        "rank": rank,
        "title": title,
        "url": url,
        "content": text,
        "tweet_id": tweet_id,
        "type": "x"
    });

    if let Some(object) = item.as_object_mut() {
        if !author.is_empty() {
            object.insert("author".to_string(), json!(author));
        }
        if let Some(username) = username {
            object.insert("username".to_string(), json!(username));
        }
        if let Some(created_at) = tweet.get("created_at").and_then(Value::as_str) {
            object.insert("created_at".to_string(), json!(created_at));
        }
        if let Some(metrics) = tweet.get("public_metrics") {
            object.insert("public_metrics".to_string(), metrics.clone());
        }
    }

    Some(item)
}
```

Add title/url helpers:

```rust
fn x_tweet_url(username: Option<&str>, tweet_id: &str) -> String {
    match username.filter(|username| !username.trim().is_empty()) {
        Some(username) => format!("https://x.com/{username}/status/{tweet_id}"),
        None => format!("https://x.com/i/web/status/{tweet_id}"),
    }
}

fn x_tweet_title(text: &str, tweet_id: &str) -> String {
    let first_line = text.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let title = first_line.chars().take(80).collect::<String>();
    if title.trim().is_empty() {
        format!("X post {tweet_id}")
    } else {
        title
    }
}
```

- [ ] **Step 4: Run helper tests to verify GREEN**

Run:

```powershell
cargo test --locked recommend::tests::reports_missing_x_bearer_token recommend::tests::prefers_x_bearer_token_over_twitter_bearer_token recommend::tests::normalizes_x_recent_search_response
```

Expected: PASS.

- [ ] **Step 5: Write failing async request/error tests**

Add tests in `src/recommend.rs`:

```rust
#[tokio::test]
async fn collect_x_recent_search_sends_bearer_query_and_fields() {
    let api_url = serve_x_recent_search_api(200, r#"{
        "data": [{
            "id": "1800000000000000001",
            "text": "Security research example",
            "author_id": "7"
        }],
        "includes": {
            "users": [{"id":"7","username":"sec","name":"Security Example"}]
        }
    }"#)
    .await;

    let items = collect_x_recent_search(&api_url, "security lang:en", 5, "token-123").await.unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["url"], "https://x.com/sec/status/1800000000000000001");
}

#[tokio::test]
async fn collect_x_recent_search_reports_http_errors() {
    let api_url = serve_x_recent_search_api(429, r#"{"title":"Too Many Requests"}"#).await;

    let error = collect_x_recent_search(&api_url, "security lang:en", 5, "token-123")
        .await
        .unwrap_err();

    assert!(error.to_string().contains("X recent search failed"));
    assert!(error.to_string().contains("429"));
}
```

Add a local test server helper:

```rust
async fn serve_x_recent_search_api(status: u16, body: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request_buffer = [0_u8; 4096];
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let bytes_read = socket.read(&mut request_buffer).await.unwrap();
        let request = String::from_utf8_lossy(&request_buffer[..bytes_read]).to_string();
        assert!(request.starts_with("GET /?"));
        assert!(request.contains("query=security"));
        assert!(request.contains("Authorization: Bearer token-123"));
        assert!(request.contains("tweet.fields="));
        assert!(request.contains("expansions=author_id"));
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

- [ ] **Step 6: Run async tests to verify RED**

Run:

```powershell
cargo test --locked recommend::tests::collect_x_recent_search_sends_bearer_query_and_fields recommend::tests::collect_x_recent_search_reports_http_errors
```

Expected: FAIL because `collect_x_recent_search` is missing.

- [ ] **Step 7: Implement async collector and dispatch**

Add `collect_x_recent_search`:

```rust
async fn collect_x_recent_search(
    api_url: &str,
    query: &str,
    limit: usize,
    bearer_token: &str,
) -> Result<Vec<Value>> {
    let client = reqwest::Client::new();
    let response = client
        .get(api_url)
        .bearer_auth(bearer_token)
        .query(&[
            ("query", query),
            ("max_results", &limit.to_string()),
            ("tweet.fields", "created_at,public_metrics,author_id,entities"),
            ("expansions", "author_id"),
            ("user.fields", "username,name"),
        ])
        .send()
        .await
        .with_context(|| format!("Failed to fetch X recent search from {api_url}"))?;

    let status = response.status();
    if !status.is_success() {
        let reset = response
            .headers()
            .get("x-rate-limit-reset")
            .and_then(|value| value.to_str().ok())
            .map(|value| format!(" x-rate-limit-reset={value}"))
            .unwrap_or_default();
        let body = response.text().await.unwrap_or_default();
        bail!(
            "X recent search failed with HTTP status {status}.{reset} Response: {}",
            body.chars().take(500).collect::<String>()
        );
    }

    let value: Value = response.json().await.context("Failed to parse X recent search response")?;
    parse_x_recent_search_response(&value, limit)
}
```

In `collect_source`, add the `SearchRequest::XRecentSearch` arm before the generic `SearchApi` fallback:

```rust
DiscoveryEndpoint::SearchApi {
    api_url,
    default_query,
    request: SearchRequest::XRecentSearch,
} => {
    let query = query_override_or_default(query, required_default_query(site_name, default_query)?);
    let token = x_bearer_token()?;
    collect_x_recent_search(api_url, query, limit, &token).await?
}
```

- [ ] **Step 8: Run collector tests to verify GREEN**

Run:

```powershell
cargo test --locked recommend::tests::collect_x_recent_search_sends_bearer_query_and_fields recommend::tests::collect_x_recent_search_reports_http_errors
```

Expected: PASS.

## Task 3: Docs, Sample Config, And Verification

**Files:**
- Modify: `article-collector.toml`
- Modify: `docs/sites/twitter.md`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-06-28-x-recent-search-recommendations.md`

- [ ] **Step 1: Update sample config**

Add `twitter` to `[recommend].sources` after `zenn`, and add:

```toml
[recommend.source.twitter]
limit = 10
query = "(AI OR Rust OR security) lang:en -is:retweet"
```

- [ ] **Step 2: Update site docs**

Update `docs/sites/twitter.md` so discovery says:

```markdown
## discovery endpoint の構造

- 種類: `DiscoveryEndpoint::SearchApi`
- endpoint: `https://api.x.com/2/tweets/search/recent`
- 認証: `X_BEARER_TOKEN` または `TWITTER_BEARER_TOKEN`
- query: `[recommend.source.twitter].query`

この discovery は X の For You timeline そのものではなく、X API v2 recent search を使った query-based recommendation source として扱う。
```

- [ ] **Step 3: Update README**

Update the supported/recommend source docs to mention:

```markdown
`twitter` / `x` は X API v2 recent search を使う recommend source として利用できる。実行時は `X_BEARER_TOKEN` または `TWITTER_BEARER_TOKEN` が必要で、検索条件は `[recommend.source.twitter].query` で指定する。これは X の For You timeline ではなく query-based recommendation discovery。
```

Also update the current `recommend all` source list to include `twitter`.

- [ ] **Step 4: Run focused tests**

Run:

```powershell
cargo test --locked sites::tests::resolves_twitter_aliases_and_recent_search_discovery recommend::tests::resolves_twitter_site_name_to_recent_search recommend::tests::twitter_source_plan_uses_config_query_and_limit recommend::tests::reports_missing_x_bearer_token recommend::tests::prefers_x_bearer_token_over_twitter_bearer_token recommend::tests::normalizes_x_recent_search_response recommend::tests::collect_x_recent_search_sends_bearer_query_and_fields recommend::tests::collect_x_recent_search_reports_http_errors
```

Expected: PASS.

- [ ] **Step 5: Run full verification**

Run:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Expected: all commands exit 0.

- [ ] **Step 6: Update Plane**

Set ACS-75 to Done after the plan is committed. Set ACS-76 to Done when implementation passes focused tests. Set ACS-77 to Done after docs and full verification pass. Keep ACS-74 In Progress until all sub-issues are done, then set it to Done.

## Self-Review

- Spec coverage: registry, config, token handling, X recent search collector, docs, and verification are covered.
- Marker scan: no unfinished markers are used.
- Type consistency: `SearchRequest::XRecentSearch`, `collect_x_recent_search`, `parse_x_recent_search_response`, and `x_bearer_token_from_env` are consistently named across tasks.
