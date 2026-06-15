# Recommend Source Config Design

## Goal

Allow `article-collector recommend all` to use source-specific TOML configuration so arXiv categories can be tuned for #news without changing code.

## Scope

- Load `article-collector.toml` from the current directory when present.
- Allow `--config <PATH>` to load an explicit TOML config.
- Configure `recommend` source order, global limit, source enabled flags, source-specific limits, and arXiv query.
- Keep `recommend all --query ...` rejected because `--query` is a direct queryable-source override.
- Add TOML formatting and lint checks locally and in GitHub Actions.

## Config Shape

```toml
[recommend]
sources = ["hackernews", "devto", "zenn", "arxiv"]
limit = 30

[recommend.source.arxiv]
limit = 10
query = "cat:cs.AI OR cat:cs.CL OR cat:cs.CV OR cat:cs.LG OR cat:cs.IR OR cat:cs.SE OR cat:stat.ML"
```

`sources` is the ordered list used by `recommend all`. Per-source tables use `recommend.source.<site>` so the TOML key `sources` is not both an array and a table.

## Precedence

CLI values override source-specific config. Source-specific config overrides global recommend config. Global config overrides code defaults. Empty query strings are ignored.

## Validation

Unknown TOML keys are parse errors. Unknown or non-recommendable source names in `recommend.sources` are command errors. `enabled = false` removes that source from `all` but does not prevent an explicit `recommend <site>` run.

## CI

Taplo is the TOML formatter/linter. Local `task lint` runs `task toml-check`; GitHub Actions runs a separate `TOML Check` job with `taplo format --check` and `taplo lint`.
