# History Clear Design

## Goal

Add a small CLI command that clears the SQLite-backed recommend history so future `recommend` runs can emit articles that were previously marked as seen.

## Scope

- Add `article-collector history clear`.
- Resolve the history database path using the same `[recommend].history_path` and default path logic used by `recommend`.
- Clear only the `recommend_seen_items` rows.
- Keep the SQLite database file and schema in place when the database exists.
- Create or initialize the database when it does not exist, then report that zero items were cleared.
- Support `--config <PATH>` on `history clear`.

Out of scope:

- A `reset` alias.
- Per-site or per-source history clearing.
- Removing generated `raw.json`, `translated.md`, or `recommended_articles/` artifacts.
- Changing recommend deduplication semantics.

## CLI

Use a new top-level `history` command with nested subcommands:

```bash
article-collector history clear
article-collector history clear --config article-collector.toml
```

The command prints the number of deleted history entries and the database path to stderr. It does not print JSON because this is an operational maintenance command, not a pipeline artifact producer.

## Data Flow

1. Parse `history clear`.
2. Load the optional config file through the existing config loader.
3. Resolve the history path through the existing recommend history path helper.
4. Open `RecommendationHistory`, which initializes the schema if needed.
5. Delete all rows from `recommend_seen_items`.
6. Print a concise status line.

## Error Handling

- If config loading fails, return the existing config error.
- If the history path cannot be resolved, return the existing default path error.
- If SQLite cannot be opened or cleared, fail the command.
- Clearing an empty or newly created DB succeeds with `0` cleared entries.

## Testing

- Unit-test `RecommendationHistory::clear_seen_items` with an in-memory database.
- CLI-test that root help lists `history`.
- CLI-test that `history clear --config <temp config>` succeeds against a temporary SQLite path and allows the same recommendation URL to become new again.
- Keep verification on Rust stable with `cargo fmt`, `cargo clippy`, and `cargo test --locked`.

## Documentation

Update README command and recommend-history sections with `history clear` usage and the note that it only clears SQLite recommend history.
