use anyhow::Context;
use article_collector::x_connector::oauth::{
    pkce_challenge, CallbackResult, OAuthClient, OAuthConfig,
};
use article_collector::x_connector::remote_mcp::XRemoteMcp;
use article_collector::x_connector::stdio::{OAuthConfigurator, StdioServer};
use article_collector::x_connector::token_store::{TokenSet, TokenStore};
use article_collector::x_connector::{
    descriptor, normalize_response, parse_collect_options, CollectOptions, CollectionMode,
};
use chrono::{Duration, Utc};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use url::Url;

fn options(value: Value) -> Map<String, Value> {
    value
        .as_object()
        .cloned()
        .expect("test options must be object")
}

#[test]
fn options_and_descriptor_describe_the_x_source_contract() {
    let descriptor = descriptor();

    assert_eq!(descriptor["name"], "x");
    assert_eq!(descriptor["protocol_version"], "1.0.0");
    assert_eq!(descriptor["supports_configuration"], true);

    let source = &descriptor["sources"][0];
    assert_eq!(source["name"], "twitter");
    assert_eq!(source["aliases"], json!(["x", "x-twitter"]));
    assert_eq!(source["options_schema"]["additionalProperties"], false);
    assert_eq!(source["options_schema"]["required"], json!(["mode"]));
}

#[test]
fn options_and_descriptor_accept_valid_search_and_alias_sources() {
    let search = parse_collect_options(
        "x",
        &options(json!({"mode": "search", "query": "Rust lang:en"})),
    )
    .unwrap();

    assert_eq!(search.mode, CollectionMode::Search);
    assert_eq!(search.query.as_deref(), Some("Rust lang:en"));

    let timeline = parse_collect_options("twitter", &options(json!({"mode": "timeline"}))).unwrap();
    assert_eq!(timeline.mode, CollectionMode::Timeline);
    assert!(timeline.query.is_none());
}

#[test]
fn options_and_descriptor_reject_invalid_mode_query_and_keys() {
    let cases = [
        json!({}),
        json!({"mode": "other"}),
        json!({"mode": "search"}),
        json!({"mode": "search", "query": "   "}),
        json!({"mode": "timeline", "query": "Rust"}),
        json!({"mode": "bookmarks", "query": "Rust"}),
        json!({"mode": "timeline", "unexpected": true}),
    ];

    for value in cases {
        let error = parse_collect_options("twitter", &options(value)).unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    let error =
        parse_collect_options("unknown", &options(json!({"mode": "timeline"}))).unwrap_err();
    assert!(error.to_string().contains("source"));
}

#[test]
fn normalizes_response_with_author_and_metrics() {
    let response = json!({
        "data": [{
            "id": "1800000000000000000",
            "text": "Rust async tooling keeps getting better\nSecond line",
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

    let items = normalize_response(&response).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Rust async tooling keeps getting better");
    assert_eq!(
        items[0].content,
        "Rust async tooling keeps getting better\nSecond line"
    );
    assert_eq!(
        items[0].url.as_str(),
        "https://x.com/alice/status/1800000000000000000"
    );
    assert_eq!(items[0].metadata["tweet_id"], "1800000000000000000");
    assert_eq!(items[0].metadata["author"], "Alice Example");
    assert_eq!(items[0].metadata["username"], "alice");
    assert_eq!(items[0].metadata["created_at"], "2026-06-28T00:00:00.000Z");
    assert_eq!(items[0].metadata["public_metrics"]["like_count"], 10);
    assert!(!items[0].metadata.contains_key("source"));
    assert!(!items[0].metadata.contains_key("site"));
    assert!(!items[0].metadata.contains_key("rank"));
}

#[test]
fn normalizes_response_with_fallback_url_and_deduplicates_post_ids() {
    let response = json!({
        "data": [
            {"id": "1", "text": "first"},
            {"id": "1", "text": "duplicate"},
            {"id": "2", "text": "second", "author_id": "missing"}
        ]
    });

    let items = normalize_response(&response).unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].url.as_str(), "https://x.com/i/web/status/1");
    assert_eq!(items[1].url.as_str(), "https://x.com/i/web/status/2");
}

#[test]
fn normalizes_response_rejects_malformed_posts() {
    for response in [
        json!({}),
        json!({"data": [{"text": "missing id"}]}),
        json!({"data": [{"id": "missing text"}]}),
        json!({"data": "not an array"}),
    ] {
        assert!(normalize_response(&response).is_err());
    }
}

#[test]
fn token_store_round_trips_and_marks_expiry_without_exposing_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let store = TokenStore::from_root(temp.path().to_path_buf());
    let tokens = TokenSet {
        access_token: "access-secret".to_string(),
        refresh_token: Some("refresh-secret".to_string()),
        expires_at: Utc::now() + Duration::minutes(5),
    };

    store.save(&tokens).unwrap();
    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded, tokens);
    assert!(!loaded.is_expired(Utc::now()));
    assert!(loaded.is_expired(Utc::now() + Duration::minutes(6)));
    let missing_store = TokenStore::from_root(temp.path().join("missing"));
    assert!(missing_store.load().unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn token_store_writes_credentials_with_restrictive_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let store = TokenStore::from_root(temp.path().to_path_buf());
    store
        .save(&TokenSet {
            access_token: "access-secret".to_string(),
            refresh_token: None,
            expires_at: Utc::now(),
        })
        .unwrap();

    let mode = std::fs::metadata(store.path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn pkce_authorization_request_contains_s256_state_and_read_scopes() {
    let config = OAuthConfig::for_test(
        "client-id",
        "https://example.com/authorize",
        "https://example.com/token",
        "http://127.0.0.1:53681/callback",
    )
    .unwrap();
    let client = OAuthClient::new(config).unwrap();
    let request = client.authorization_request().unwrap();
    let query: std::collections::HashMap<_, _> = request.url.query_pairs().into_owned().collect();

    assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
    assert_eq!(
        query.get("client_id").map(String::as_str),
        Some("client-id")
    );
    assert_eq!(
        query.get("redirect_uri").map(String::as_str),
        Some("http://127.0.0.1:53681/callback")
    );
    assert_eq!(
        query.get("scope").map(String::as_str),
        Some("tweet.read users.read bookmark.read offline.access")
    );
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    assert_eq!(
        query.get("code_challenge").map(String::as_str),
        Some(request.challenge.as_str())
    );
    assert_eq!(
        query.get("state").map(String::as_str),
        Some(request.state.as_str())
    );
    assert_eq!(pkce_challenge(&request.verifier), request.challenge);
}

#[test]
fn oauth_callback_rejects_state_mismatch_and_denial_without_returning_code() {
    let config = OAuthConfig::for_test(
        "client-id",
        "https://example.com/authorize",
        "https://example.com/token",
        "http://127.0.0.1:53681/callback",
    )
    .unwrap();
    let client = OAuthClient::new(config).unwrap();

    let mismatch = client.parse_callback(
        &Url::parse("http://127.0.0.1:53681/callback?state=wrong&code=secret-code").unwrap(),
        "expected",
    );
    assert!(matches!(
        mismatch,
        Err(CallbackResult::StateMismatch { .. })
    ));

    let denied = client.parse_callback(
        &Url::parse("http://127.0.0.1:53681/callback?state=expected&error=access_denied").unwrap(),
        "expected",
    );
    let error = denied.unwrap_err();
    assert!(matches!(error, CallbackResult::Denied { .. }));
    assert!(!error.to_string().contains("secret-code"));
}

#[tokio::test]
async fn remote_mcp_collects_search_results_with_pagination() -> anyhow::Result<()> {
    let server = FakeXMcpServer::start().await?;
    let client = XRemoteMcp::new(
        Url::parse(&server.url())?,
        TokenSet {
            access_token: "test-access-token".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::minutes(5),
        },
    )?;
    let items = client
        .collect(
            &CollectOptions {
                mode: CollectionMode::Search,
                query: Some("Rust lang:en".to_string()),
            },
            2,
        )
        .await?;

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].metadata["tweet_id"], "1");
    assert_eq!(items[1].metadata["tweet_id"], "2");
    assert_eq!(items[0].url.as_str(), "https://x.com/alice/status/1");

    let calls = server.calls().await;
    assert_eq!(
        calls,
        vec![
            (
                "searchPostsRecent".to_string(),
                json!({
                    "query": "Rust lang:en",
                    "max_results": 2,
                    "expansions": "author_id",
                    "tweet.fields": "author_id,created_at,public_metrics",
                    "user.fields": "name,username"
                })
            ),
            (
                "searchPostsRecent".to_string(),
                json!({
                    "query": "Rust lang:en",
                    "max_results": 1,
                    "next_token": "page-2",
                    "expansions": "author_id",
                    "tweet.fields": "author_id,created_at,public_metrics",
                    "user.fields": "name,username"
                })
            )
        ]
    );

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn remote_mcp_runs_timeline_and_bookmark_tool_sequences() -> anyhow::Result<()> {
    for (scenario, mode, expected_tool, expected_id) in [
        (
            FakeScenario::Timeline,
            CollectionMode::Timeline,
            "getUsersTimeline",
            "3",
        ),
        (
            FakeScenario::Bookmarks,
            CollectionMode::Bookmarks,
            "getUsersBookmarks",
            "4",
        ),
    ] {
        let server = FakeXMcpServer::start_with(scenario).await?;
        let client = test_remote_client(&server)?;
        let items = client
            .collect(&CollectOptions { mode, query: None }, 1)
            .await?;

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].metadata["tweet_id"], expected_id);
        let calls = server.calls().await;
        assert_eq!(calls[0].0, "getUsersMe");
        assert_eq!(calls[1].0, expected_tool);
        assert_eq!(calls[1].1["id"], "42");
        assert_eq!(calls[1].1["max_results"], 1);
        server.shutdown().await;
    }
    Ok(())
}

#[tokio::test]
async fn remote_mcp_deduplicates_across_pages_and_fails_closed() -> anyhow::Result<()> {
    let server = FakeXMcpServer::start_with(FakeScenario::Duplicate).await?;
    let client = test_remote_client(&server)?;
    let items = client
        .collect(
            &CollectOptions {
                mode: CollectionMode::Search,
                query: Some("Rust".to_string()),
            },
            2,
        )
        .await?;
    assert_eq!(
        items
            .iter()
            .map(|item| item.metadata["tweet_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["1", "2"]
    );
    server.shutdown().await;

    let server = FakeXMcpServer::start_with(FakeScenario::MissingTool).await?;
    let client = test_remote_client(&server)?;
    let error = client.validate_tools().await.unwrap_err();
    assert!(error.to_string().contains("getUsersBookmarks"));
    server.shutdown().await;

    let server = FakeXMcpServer::start_with(FakeScenario::Malformed).await?;
    let client = test_remote_client(&server)?;
    let error = client
        .collect(
            &CollectOptions {
                mode: CollectionMode::Search,
                query: Some("Rust".to_string()),
            },
            1,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("malformed"));
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn remote_mcp_refreshes_once_on_401_and_maps_permission_errors() -> anyhow::Result<()> {
    let server = FakeXMcpServer::start_with(FakeScenario::UnauthorizedOnce).await?;
    let temp = tempfile::tempdir()?;
    let store = TokenStore::from_root(temp.path().to_path_buf());
    let oauth = OAuthClient::new(OAuthConfig::for_test(
        "client-id",
        &format!("{}/authorize", server.base_url()),
        &format!("{}/token", server.base_url()),
        "http://127.0.0.1:53681/callback",
    )?)?;
    let client = XRemoteMcp::new(
        Url::parse(&server.url())?,
        TokenSet {
            access_token: "expired-access".to_string(),
            refresh_token: Some("old-refresh".to_string()),
            expires_at: Utc::now() + Duration::minutes(5),
        },
    )?
    .with_refresh(oauth, store.clone());
    let items = client
        .collect(
            &CollectOptions {
                mode: CollectionMode::Search,
                query: Some("Rust".to_string()),
            },
            1,
        )
        .await?;
    assert_eq!(items.len(), 1);
    assert_eq!(store.load()?.unwrap().access_token, "refreshed-access");
    server.shutdown().await;

    for (scenario, expected) in [
        (FakeScenario::Forbidden, "scope"),
        (FakeScenario::RateLimited, "rate limit"),
    ] {
        let server = FakeXMcpServer::start_with(scenario).await?;
        let client = test_remote_client(&server)?;
        let error = client.validate_tools().await.unwrap_err();
        assert!(error.to_string().contains(expected));
        server.shutdown().await;
    }
    Ok(())
}

#[tokio::test]
async fn stdio_server_handles_protocol_tools_and_collect() -> anyhow::Result<()> {
    let server = FakeXMcpServer::start().await?;
    let temp = tempfile::tempdir()?;
    let store = TokenStore::from_root(temp.path().to_path_buf());
    store.save(&TokenSet {
        access_token: "stdio-access".to_string(),
        refresh_token: None,
        expires_at: Utc::now() + Duration::minutes(5),
    })?;
    let app = StdioServer::new(
        Url::parse(&server.url())?,
        store.clone(),
        None,
        Arc::new(TestConfigurator),
    );

    let initialize = app
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}
        }))
        .await?
        .unwrap();
    assert_eq!(initialize["result"]["protocolVersion"], "2025-06-18");

    assert!(app
        .handle_message(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .await?
        .is_none());

    let tools = app
        .handle_message(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}))
        .await?
        .unwrap();
    let tool_names = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        tool_names,
        vec![
            "article_collector_describe_sources",
            "article_collector_collect",
            "article_collector_configure"
        ]
    );

    let descriptor = app
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "article_collector_describe_sources", "arguments": {}}
        }))
        .await?
        .unwrap();
    assert_eq!(descriptor["result"]["structuredContent"]["name"], "x");

    let collected = app
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "article_collector_collect",
                "arguments": {"source": "twitter", "limit": 1, "options": {"mode": "search", "query": "Rust"}}
            }
        }))
        .await?
        .unwrap();
    assert_eq!(
        collected["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let configured = app
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {"name": "article_collector_configure", "arguments": {}}
        }))
        .await?
        .unwrap();
    assert_eq!(
        configured["result"]["structuredContent"]["status"],
        "configured"
    );

    let invalid = app
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "article_collector_collect",
                "arguments": {"source": "twitter", "limit": 1, "options": {"mode": "timeline", "query": "Rust"}}
            }
        }))
        .await?
        .unwrap();
    assert_eq!(invalid["error"]["code"], -32602);

    let secret_app = StdioServer::new(
        Url::parse(&server.url())?,
        store,
        None,
        Arc::new(SecretConfigurator),
    );
    let secret_error = secret_app
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {"name": "article_collector_configure", "arguments": {}}
        }))
        .await?
        .unwrap();
    let secret_error_text = secret_error.to_string();
    assert!(secret_error_text.contains("secret-like"));
    assert!(!secret_error_text.contains("should-not-leak"));

    server.shutdown().await;
    Ok(())
}

struct TestConfigurator;

#[async_trait::async_trait]
impl OAuthConfigurator for TestConfigurator {
    async fn configure(&self) -> anyhow::Result<Value> {
        Ok(json!({"status": "configured"}))
    }
}

struct SecretConfigurator;

#[async_trait::async_trait]
impl OAuthConfigurator for SecretConfigurator {
    async fn configure(&self) -> anyhow::Result<Value> {
        Ok(json!({"access_token": "should-not-leak"}))
    }
}

fn test_remote_client(server: &FakeXMcpServer) -> anyhow::Result<XRemoteMcp> {
    XRemoteMcp::new(
        Url::parse(&server.url())?,
        TokenSet {
            access_token: "test-access-token".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::minutes(5),
        },
    )
}

struct FakeXMcpServer {
    address: std::net::SocketAddr,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Copy)]
enum FakeScenario {
    Search,
    Timeline,
    Bookmarks,
    Duplicate,
    MissingTool,
    Malformed,
    UnauthorizedOnce,
    Forbidden,
    RateLimited,
}

impl FakeXMcpServer {
    async fn start() -> anyhow::Result<Self> {
        Self::start_with(FakeScenario::Search).await
    }

    async fn start_with(scenario: FakeScenario) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (shutdown, mut shutdown_rx) = oneshot::channel();
        let calls_for_task = Arc::clone(&calls);
        let unauthorized_seen = Arc::new(AtomicBool::new(false));
        let unauthorized_seen_for_task = Arc::clone(&unauthorized_seen);
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        if handle_fake_request(
                            stream,
                            &calls_for_task,
                            scenario,
                            &unauthorized_seen_for_task,
                        ).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            address,
            calls,
            shutdown: Some(shutdown),
            task,
        })
    }

    fn url(&self) -> String {
        format!("http://{}/mcp", self.address)
    }

    async fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().await.clone()
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

async fn handle_fake_request(
    mut stream: TcpStream,
    calls: &Arc<Mutex<Vec<(String, Value)>>>,
    scenario: FakeScenario,
    unauthorized_seen: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let (method, path, body) = read_http_request(&mut stream).await?;
    if path == "/token" {
        write_http_response(
            &mut stream,
            200,
            Some(json!({
                "access_token": "refreshed-access",
                "refresh_token": "refreshed-refresh",
                "expires_in": 3600
            })),
        )
        .await?;
        return Ok(());
    }
    if matches!(scenario, FakeScenario::UnauthorizedOnce)
        && !unauthorized_seen.swap(true, Ordering::SeqCst)
    {
        write_http_response(&mut stream, 401, None).await?;
        return Ok(());
    }
    if matches!(scenario, FakeScenario::Forbidden) {
        write_http_response(&mut stream, 403, None).await?;
        return Ok(());
    }
    if matches!(scenario, FakeScenario::RateLimited) {
        write_http_response(&mut stream, 429, None).await?;
        return Ok(());
    }
    let request: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)?
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let rpc_method = request.get("method").and_then(Value::as_str).unwrap_or("");

    if rpc_method == "notifications/initialized" {
        write_http_response(&mut stream, 202, None).await?;
        return Ok(());
    }

    let response = match rpc_method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "fake-x", "version": "1.0.0"}
            }
        }),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"tools": fake_tools(scenario)}
        }),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_default();
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            calls.lock().await.push((name.clone(), arguments.clone()));
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"content": [{"type": "text", "text": serde_json::to_string(&fake_tool_result(scenario, &name, &arguments))?}], "isError": false}
            })
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("unknown method {rpc_method}")}
        }),
    };

    let _ = method;
    write_http_response(&mut stream, 200, Some(response)).await?;
    Ok(())
}

fn fake_tools(scenario: FakeScenario) -> Value {
    let mut tools = vec![
        fake_tool("searchPostsRecent", &["query", "max_results"]),
        fake_tool("getUsersMe", &["user.fields"]),
        fake_tool("getUsersTimeline", &["id", "max_results"]),
        fake_tool("getUsersBookmarks", &["id", "max_results"]),
    ];
    if matches!(scenario, FakeScenario::MissingTool) {
        tools.pop();
    }
    Value::Array(tools)
}

fn fake_tool(name: &str, properties: &[&str]) -> Value {
    let properties = properties
        .iter()
        .map(|property| ((*property).to_string(), json!({"type": "string"})))
        .collect::<Map<_, _>>();
    json!({
        "name": name,
        "inputSchema": {"type": "object", "properties": properties}
    })
}

fn fake_tool_result(scenario: FakeScenario, name: &str, arguments: &Value) -> Value {
    if matches!(scenario, FakeScenario::Malformed) && name == "searchPostsRecent" {
        return json!({"data": "malformed"});
    }
    match (scenario, name) {
        (FakeScenario::Search, "searchPostsRecent")
        | (FakeScenario::UnauthorizedOnce, "searchPostsRecent")
            if arguments.get("next_token").is_none() =>
        {
            json!({
                "data": [{"id": "1", "text": "first result", "author_id": "42"}],
                "includes": {"users": [{"id": "42", "username": "alice", "name": "Alice"}]},
                "meta": {"next_token": "page-2"}
            })
        }
        (FakeScenario::Search, "searchPostsRecent")
        | (FakeScenario::UnauthorizedOnce, "searchPostsRecent") => json!({
            "data": [{"id": "2", "text": "second result"}],
            "meta": {}
        }),
        (FakeScenario::Duplicate, "searchPostsRecent") if arguments.get("next_token").is_none() => {
            json!({
                "data": [
                    {"id": "1", "text": "first result"},
                    {"id": "1", "text": "duplicate result"}
                ],
                "meta": {"next_token": "page-2"}
            })
        }
        (FakeScenario::Duplicate, "searchPostsRecent") => json!({
            "data": [
                {"id": "1", "text": "duplicate result"},
                {"id": "2", "text": "second result"}
            ],
            "meta": {}
        }),
        (FakeScenario::Timeline, "getUsersMe") | (FakeScenario::Bookmarks, "getUsersMe") => {
            json!({"data": {"id": "42"}})
        }
        (FakeScenario::Timeline, "getUsersTimeline") => {
            json!({"data": [{"id": "3", "text": "timeline result"}], "meta": {}})
        }
        (FakeScenario::Bookmarks, "getUsersBookmarks") => {
            json!({"data": [{"id": "4", "text": "bookmark result"}], "meta": {}})
        }
        _ => json!({"data": [], "meta": {}}),
    }
}

async fn read_http_request(stream: &mut TcpStream) -> anyhow::Result<(String, String, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("fake MCP server received an incomplete HTTP request")
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index;
        }
    };
    let header_bytes = &bytes[..header_end];
    let header_text = std::str::from_utf8(header_bytes)?;
    let mut lines = header_text.lines();
    let request_line = lines
        .next()
        .context("fake MCP request has no request line")?;
    let method = request_line
        .split_whitespace()
        .next()
        .context("fake MCP request has no HTTP method")?
        .to_string();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("fake MCP request has no path")?
        .to_string();
    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.eq_ignore_ascii_case("content-length")).then(|| value.trim().parse::<usize>())
        })
        .transpose()?
        .unwrap_or(0);
    let body_start = header_end + 4;
    while bytes.len() < body_start + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("fake MCP server received an incomplete HTTP body")
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok((
        method,
        path,
        bytes[body_start..body_start + content_length].to_vec(),
    ))
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    body: Option<Value>,
) -> anyhow::Result<()> {
    let body = body
        .map(|value| serde_json::to_vec(&value))
        .transpose()?
        .unwrap_or_default();
    let status_text = match status {
        200 => "OK",
        202 => "Accepted",
        401 => "Unauthorized",
        403 => "Forbidden",
        429 => "Too Many Requests",
        _ => "Error",
    };
    let auth_header = match status {
        401 => "WWW-Authenticate: Bearer realm=\"x\"\r\n",
        403 => "WWW-Authenticate: Bearer error=\"insufficient_scope\", scope=\"tweet.read\"\r\n",
        429 => "X-RateLimit-Reset: 1234567890\r\n",
        _ => "",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\n{auth_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await?;
    Ok(())
}
