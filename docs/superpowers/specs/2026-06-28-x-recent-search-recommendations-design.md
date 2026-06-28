# X Recent Search Recommendations Design

## Goal

Add X/Twitter as a recommend source so `article-collector recommend twitter` and configured `recommend all` runs can collect recent X posts that match a configurable recommendation query.

## Scope

- Keep existing direct tweet URL fetch behavior unchanged.
- Register `twitter` / `x` / `x-twitter` as a recommend-capable site.
- Use X API v2 recent search as the discovery backend.
- Read the app-only bearer token from `X_BEARER_TOKEN`, with `TWITTER_BEARER_TOKEN` as a compatibility fallback.
- Allow `[recommend.source.twitter].query` to tune the recommendation query.
- Include `twitter` in the sample config so scheduled `recommend all --config article-collector.toml` can opt into the source.
- Document that this is query-based recommendation discovery, not the X "For You" timeline.

## Out Of Scope

- Browser scraping of the X "For You" feed.
- User-authenticated home timeline collection.
- Posting, liking, following, or mutating X data.
- Committing any secret token or requiring a token in config files.

## Config Shape

```toml
[recommend]
sources = ["hackernews", "devto", "zenn", "twitter"]

[recommend.source.twitter]
limit = 10
query = "(AI OR Rust OR security) lang:en -is:retweet"
```

The source uses the existing precedence model: CLI limit overrides source limit, source limit overrides global limit, and source query overrides the built-in default query. Empty query strings are ignored and fall back to the default query.

## Architecture

`src/sites/twitter.rs` remains the single registry entry for X/Twitter. Its direct URL rules continue to route `x.com/<user>/status/<id>` and `twitter.com/<user>/status/<id>` to `FetchRoute::SocialStatus`; the new discovery endpoint only affects site-name and `all` recommendation flows.

`src/recommend.rs` adds a source-specific collector for the X recent search endpoint. It builds a request to `https://api.x.com/2/tweets/search/recent`, adds tweet fields, author expansion, and user fields, then normalizes API responses into the existing recommendation item shape.

The normalized item should include:

- `source` and `site`: `twitter`
- `title`: first text line, truncated to a readable title
- `url`: `https://x.com/<username>/status/<tweet_id>` when username is available, otherwise `https://x.com/i/web/status/<tweet_id>`
- `content`: tweet text
- `author`, `username`, `created_at`, `tweet_id`, and `public_metrics` when present

## Error Handling

Missing bearer tokens fail with an actionable error naming `X_BEARER_TOKEN` and `TWITTER_BEARER_TOKEN`.

HTTP errors from X fail the source with status code and response body excerpt. Rate-limit errors should include reset information when X sends it in response headers.

Empty successful responses keep the existing recommendation behavior: the source returns no items, and higher-level empty-result checks decide whether the command fails.

## Testing

Add tests before implementation for:

- `twitter` resolving to a recommendation source.
- `twitter` appearing in `recommendable_site_names()`.
- Existing direct tweet URL routing still using `FetchRoute::SocialStatus`.
- Source plan query/limit config for `[recommend.source.twitter]`.
- Missing token error text.
- Successful X response normalization, including author expansion and tweet URL construction.
- HTTP error reporting for non-success X responses.

Run the normal repo verification before completion:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

## Plane Tracking

Plane issue ACS-74 tracks the parent work. ACS-75 covers design and planning, ACS-76 covers implementation, and ACS-77 covers docs and verification.
