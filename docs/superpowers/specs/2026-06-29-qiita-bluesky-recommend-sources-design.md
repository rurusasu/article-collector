# Qiita And Bluesky Recommend Sources Design

## Goal

Add Qiita and Bluesky as recommend sources so `article-collector recommend qiita`, `article-collector recommend bluesky`, and configured `recommend all` runs can collect public technical posts from both services.

## Scope

- Register `qiita` / `qiita.com` as a recommend-capable site.
- Register `bluesky` / `bsky` / `bsky.app` as a recommend-capable site.
- Include both sources in the sample `article-collector.toml` source list.
- Use Qiita API v2 public item search as the Qiita discovery backend.
- Use the public Bluesky AppView `app.bsky.feed.searchPosts` endpoint as the Bluesky discovery backend.
- Allow `[recommend.source.qiita].query` and `[recommend.source.bluesky].query` to tune each recommendation query.
- Keep direct article/post URL fetching conservative: Qiita article URLs use generic web fetch, and Bluesky post URLs use generic web fetch unless a dedicated social fetch route is added later.
- Update README and site docs so recommend support and configuration stay truthful.

## Out Of Scope

- Browser scraping of logged-in feeds or timelines.
- Mutating Qiita or Bluesky data.
- OAuth, app passwords, sessions, or any committed secret.
- A dedicated Bluesky post fetch route. The first iteration only needs recommendation discovery and generic URL fetch compatibility.
- Full-text extraction beyond the existing generic web fetch path.

## Config Shape

```toml
[recommend]
sources = [
  "hackernews",
  "devto",
  "zenn",
  "twitter",
  "qiita",
  "bluesky",
  "arxiv",
]

[recommend.source.qiita]
limit = 10
query = "AI OR Rust OR security"

[recommend.source.bluesky]
limit = 10
query = "AI OR Rust OR security"
```

The sources use the existing precedence model: CLI limit overrides source limit, source limit overrides global limit, and source query overrides the built-in default query. Empty query strings are ignored and fall back to the built-in default query.

## Architecture

Add `src/sites/qiita.rs` and `src/sites/bluesky.rs` as the single registry entries for the new sources. `src/sites/registry.rs` should place them near the other public web/social sources, after `twitter` and before `arxiv`, so `recommendable_site_names()` and configured defaults stay predictable.

Qiita should use `DiscoveryEndpoint::SearchApi` with a source-specific request variant, such as `SearchRequest::QiitaItems`, because response normalization is site-specific even though query assembly is simple. The collector should call `https://qiita.com/api/v2/items` with `page=1`, `per_page=<limit>`, and `query=<query>`, then normalize the response into existing recommendation item JSON.

Bluesky should use a source-specific search request variant, such as `SearchRequest::BlueskySearchPosts`, because the response shape is not a simple array and post URLs must be built from author handles and record keys. The collector should call `https://api.bsky.app/xrpc/app.bsky.feed.searchPosts` with `q=<query>`, `limit=<limit>`, and `sort=latest`, then normalize the `posts` array into existing recommendation item JSON.

Qiita normalized items should include:

- `source` and `site`: `qiita`
- `title`
- `url`
- `content`: rendered body, markdown body, or title/summary fallback
- `author` or `username` when present
- `tags` when present
- `likes_count`, `created_at`, and `updated_at` when present

Bluesky normalized items should include:

- `source` and `site`: `bluesky`
- `title`: first post text line, truncated to a readable title
- `url`: `https://bsky.app/profile/<handle>/post/<rkey>` when handle and rkey are available
- `content`: post text
- `author`, `handle`, `did`, `created_at`, and like/repost/reply counts when present

## Error Handling

Qiita and Bluesky are public unauthenticated sources in this iteration, so missing token errors are not part of the behavior.

HTTP errors should fail the source with the status code and a short response body excerpt. Rate-limit responses should preserve the non-zero command failure instead of producing a partial success summary.

Successful empty responses keep the existing recommendation behavior: the source returns no items, and higher-level empty-result checks decide whether the command fails.

Malformed individual records should be skipped when they lack a usable URL. A malformed response body should fail the source with an actionable parse error.

## Testing

Add tests before implementation for:

- `qiita` and `bluesky` resolving by name and alias.
- Both sources appearing in `recommendable_site_names()` and `recommendable_sites()` in the expected order.
- Qiita and Bluesky source plan query/limit config.
- Qiita search URL/query construction.
- Qiita response normalization, including tags and author fields.
- Bluesky search URL/query construction.
- Bluesky response normalization, including URL construction from `uri` plus author handle.
- HTTP error reporting for non-success Qiita and Bluesky responses.
- Existing direct X/Twitter routing and existing source tests continuing to pass.

Run the normal repo verification before completion:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

## Plane Tracking

Before implementation starts, create or update a Plane issue for this work. The issue must include purpose, implementation scope, acceptance criteria, verification commands, and any release or cleanup work. If the implementation plan has multiple tasks, represent them as sub-issues or checklist-equivalent work units before editing production code.
