# Recommend SQLite Dedup Design

## Goal

Add persistent deduplication to `article-collector recommend` so articles that were already emitted by a previous recommend run are not emitted again.

## Scope

- Store recommend history in SQLite.
- Keep `raw.json` as the per-run output consumed by `translate`.
- Treat only items written to the current `raw.json` as seen.
- Exclude previously seen items from later recommend runs.
- Exit non-zero with `No new recommended articles found for <target>` when every collected candidate was already seen.
- Use a stable default history database location outside `ARTICLE_COLLECTOR_OUTDIR`.
- Allow the history database path to be overridden from TOML config.

Out of scope:

- Removing `raw.json`.
- Marking articles as seen only after translation succeeds.
- Recommending already-seen articles through a new CLI option.
- Aggressive URL canonicalization that could merge distinct articles.

## Dependency

Use `rusqlite` with bundled SQLite:

```toml
rusqlite = { version = "0.40", features = ["bundled"] }
```

`rusqlite` is enough because deduplication needs small synchronous local queries such as `SELECT EXISTS` and `INSERT OR IGNORE`. `sqlx` and `libsql` are heavier than needed for a local history index. A migration crate is not needed initially; schema setup can use `CREATE TABLE IF NOT EXISTS` plus `PRAGMA user_version`.

## Configuration

Extend `[recommend]` with an optional history path:

```toml
[recommend]
sources = ["hackernews", "devto", "zenn", "arxiv"]
limit = 30
history_path = "D:/article-collector-data/recommend-history.sqlite"
```

If `history_path` is omitted, use `dirs::config_dir().join("article-collector").join("recommend-history.sqlite")`. On Windows this resolves under `%APPDATA%` for normal desktop sessions. If `dirs::config_dir()` returns `None`, fail with a clear error telling the user to set `[recommend].history_path`.

`ARTICLE_COLLECTOR_OUTDIR` continues to control only run artifacts such as `raw.json` and `translated.md`.

## Schema

Create one table for seen recommend items:

```sql
CREATE TABLE IF NOT EXISTS recommend_seen_items (
  dedupe_key TEXT PRIMARY KEY,
  canonical_url TEXT NOT NULL,
  original_url TEXT NOT NULL,
  source TEXT NOT NULL,
  site TEXT,
  title TEXT,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL
);
```

`dedupe_key` is the normalized URL key used for duplicate detection. `canonical_url` is the normalized URL stored for inspection, and `original_url` preserves the URL as emitted by the source.

## URL Keying

Build `dedupe_key` from the item `url`.

Initial normalization is conservative:

- Trim whitespace.
- Lowercase scheme and host.
- Remove fragment.
- Preserve query parameters.
- Preserve path except for URL parser normalization.

Do not strip arbitrary query parameters initially. Tracking parameters such as `utm_*` can be stripped later after tests prove no supported source needs them to distinguish articles.

Items without a usable `url` are already excluded by source normalizers and should remain excluded.

## Data Flow

1. Resolve the recommend target and collect candidate items using the existing source functions.
2. Open and initialize the SQLite history store.
3. For each candidate item, derive its `dedupe_key`.
4. Drop items whose `dedupe_key` already exists in `recommend_seen_items`.
5. Preserve source order and per-source ranking among new items.
6. If no new items remain, return `No new recommended articles found for <target>` and do not write a misleading success result.
7. Write new items to `raw.json`.
8. Insert only the items written to `raw.json` into `recommend_seen_items`.
9. Continue translation for `recommend all` using the existing `raw.json` path.

This keeps SQLite as the persistent history index and keeps `raw.json` as the current-run data contract.

## Error Handling

- If the history database cannot be opened or initialized, fail the command.
- If an item URL cannot be parsed for normalization, skip that item rather than marking an unstable key.
- If a source returns no candidates, keep the current source failure behavior.
- If all valid candidates are already seen, fail with `No new recommended articles found for <target>`.
- Use `INSERT OR IGNORE` or equivalent unique-key handling so accidental concurrent or repeated inserts do not corrupt history.

## Testing

Add focused unit tests for:

- URL dedupe key normalization.
- SQLite schema initialization against an in-memory database.
- Filtering seen items while preserving unseen items.
- Inserting only emitted items into the history table.
- All-seen results becoming an error.
- Config parsing for `[recommend].history_path`.

Keep existing live-source tests separate from the SQLite history tests so unit tests do not require network access.

## Documentation

Update README command/config sections to state:

- `recommend` writes only newly seen items to `raw.json`.
- SQLite keeps the persistent recommend history.
- `history_path` can override the default database location.
- All-seen runs exit non-zero.
