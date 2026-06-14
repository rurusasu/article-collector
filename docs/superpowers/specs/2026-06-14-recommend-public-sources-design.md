# Recommend Public Sources Design

## Goal

Expand `article-collector recommend all` beyond Hacker News and Dev.to by adding unauthenticated, stable public sources for Zenn and arXiv. YouTube remains out of scope for this phase.

## Scope

- Keep existing Hacker News and Dev.to behavior.
- Add Zenn trend RSS as a recommend source.
- Add arXiv API search as a recommend source with a default AI/ML/CV/NLP query.
- Preserve the current `--limit` meaning: maximum items per configured source.
- Allow `article-collector recommend arxiv --query "<arxiv query>" --limit N`.
- Do not add YouTube in this phase; channel RSS and API-key search are a later phase.

## arXiv Default Query

The default arXiv source should target:

- `cat:cs.AI`
- `cat:cs.CL`
- `cat:cs.CV`
- `cat:cs.LG`
- `cat:stat.ML`

The default sort should be `submittedDate` descending so recommendations emphasize new papers.

## Architecture

`sites.rs` remains the registry of recommend-capable sites. `RecommendSource` gains source variants for Zenn RSS and arXiv API. `recommend.rs` keeps the source-specific collection functions and normalizes all outputs into the existing `raw.json` item shape with `source`, `site`, `title`, `url`, and `content`.

RSS and Atom are parsed with `quick-xml`, not ad hoc string matching. Zenn RSS and arXiv Atom use small source-specific normalizers because their metadata fields differ.

## Error Handling

Invalid `--query` use is rejected only when it is required by the source. Network and parse failures keep the existing fail-fast behavior for a source. Empty source results still fail, matching the current `collect_all_sources` behavior.

## Testing

Add unit tests for registry membership, arXiv target resolution, Zenn RSS item parsing, arXiv Atom item parsing, and default arXiv query construction. Keep the existing live integration test for all registered sources.
