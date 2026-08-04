use article_collector::x_connector::{
    descriptor, normalize_response, parse_collect_options, CollectionMode,
};
use serde_json::{json, Map, Value};

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
