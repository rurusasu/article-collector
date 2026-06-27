# Recommend Index Summary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand `recommend` generated `translated.md` so it can be copied into Slack-like channels as a concise article list with metadata and summary excerpts.

**Architecture:** Keep the behavior localized to `src/recommend_artifacts.rs`, where the translated index is already rendered. Add small pure helpers for source metadata, Markdown excerpt extraction, and fallback summary selection, then document the operational Slack posting workflow in `README.md`.

**Tech Stack:** Rust stable, `serde_json::Value`, existing Markdown artifacts, existing README command docs.

---

### Task 1: Translated Index Rendering

**Files:**
- Modify: `src/recommend_artifacts.rs`

- [ ] **Step 1: Write failing tests**

Add tests under `recommend_artifacts::tests` that expect:

```rust
#[test]
fn writes_translated_index_with_slack_ready_metadata_and_summary_excerpt() {
    let outdir = temp_outdir("translated-index-summary");
    let translated_path = outdir.join("recommended_articles/001-hackernews-example_translated.md");
    fs::create_dir_all(translated_path.parent().unwrap()).unwrap();
    fs::write(
        &translated_path,
        "## Example Article\n\nSource: hackernews\n\nこれは推薦記事の翻訳済み本文です。Slack に貼るための短い抜粋として使います。\n\n続きの本文。",
    )
    .unwrap();
    let artifacts = vec![ArticleArtifact {
        item: json!({
            "title": "Example Article",
            "source": "hackernews",
            "url": "https://example.com/article",
            "hn_url": "https://news.ycombinator.com/item?id=123",
            "score": 43,
            "comments": 7,
            "description": "Fallback description"
        }),
        json_path: outdir.join("recommended_articles/001-hackernews-example.json"),
        translated_path: Some(translated_path),
    }];

    write_translated_index(&outdir, "all", &artifacts, false).unwrap();

    let index = fs::read_to_string(outdir.join("translated.md")).unwrap();
    assert!(index.contains("1. Example Article"));
    assert!(index.contains("   URL: https://example.com/article"));
    assert!(index.contains("   Hacker News: https://news.ycombinator.com/item?id=123"));
    assert!(index.contains("   Score: 43"));
    assert!(index.contains("   Comments: 7"));
    assert!(index.contains("   Summary: これは推薦記事の翻訳済み本文です。Slack に貼るための短い抜粋として使います。"));
    assert!(index.contains("   Translation: recommended_articles/001-hackernews-example_translated.md"));
}
```

Add a second test for fallback metadata:

```rust
#[test]
fn writes_translated_index_summary_from_metadata_when_translation_excerpt_is_unavailable() {
    let outdir = temp_outdir("translated-index-summary-fallback");
    let artifacts = vec![ArticleArtifact {
        item: json!({
            "title": "Advisory Example",
            "source": "github-advisory",
            "url": "https://github.com/advisories/GHSA-abcd",
            "summary": "Patch the affected dependency before deploying the next release.",
            "severity": "high",
            "cve_id": "CVE-2026-0001",
            "ghsa_id": "GHSA-abcd"
        }),
        json_path: outdir.join("recommended_articles/001-github-advisory-example.json"),
        translated_path: None,
    }];

    write_translated_index(&outdir, "github-advisory", &artifacts, false).unwrap();

    let index = fs::read_to_string(outdir.join("translated.md")).unwrap();
    assert!(index.contains("1. Advisory Example"));
    assert!(index.contains("   URL: https://github.com/advisories/GHSA-abcd"));
    assert!(index.contains("   Severity: high"));
    assert!(index.contains("   CVE: CVE-2026-0001"));
    assert!(index.contains("   GHSA: GHSA-abcd"));
    assert!(index.contains("   Summary: Patch the affected dependency before deploying the next release."));
    assert!(!index.contains("Translation:"));
}
```

- [ ] **Step 2: Verify tests fail**

Run:

```powershell
cargo test --locked recommend_artifacts::tests::writes_translated_index_with_slack_ready_metadata_and_summary_excerpt recommend_artifacts::tests::writes_translated_index_summary_from_metadata_when_translation_excerpt_is_unavailable
```

Expected: FAIL because the current index uses Markdown links and does not render summary/source metadata fields.

- [ ] **Step 3: Implement minimal rendering helpers**

Add helpers in `src/recommend_artifacts.rs`:

- `push_item_metadata`
- `summary_excerpt`
- `translation_excerpt`
- `metadata_excerpt`
- `truncate_excerpt`
- `string_or_number_field`

Update `write_translated_index` so it lists every artifact, not only translated artifacts, and renders `Title`, metadata, optional `Summary`, and optional `Translation`.

- [ ] **Step 4: Verify focused tests pass**

Run:

```powershell
cargo test --locked recommend_artifacts::tests
```

Expected: PASS.

### Task 2: README Slack Workflow

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add docs**

Document that `translated.md` is now Slack-ready and can be posted manually or via tools that read Markdown. Include a PowerShell example:

```powershell
$env:ARTICLE_COLLECTOR_TEMP_DIR = "$PWD/.tmp/recommend-slack"
article-collector recommend all --limit 5 --config article-collector.toml
Get-Content "$env:ARTICLE_COLLECTOR_TEMP_DIR/translated.md" -Raw
```

Mention that summary excerpts prefer translated article text and fall back to source metadata.

- [ ] **Step 2: Verify docs mention Slack and translated index**

Run:

```powershell
rg -n "Slack|translated.md|Summary" README.md
```

Expected: output includes the new posting workflow section.

### Task 3: Full Verification And Plane

**Files:**
- Update: Plane issue `ACS-72`

- [ ] **Step 1: Format**

Run:

```powershell
cargo fmt --check
```

- [ ] **Step 2: Test**

Run:

```powershell
cargo test --locked
```

- [ ] **Step 3: Lint**

Run:

```powershell
cargo clippy --locked --all-targets --all-features -- -D warnings
```

- [ ] **Step 4: Update Plane status**

After the implementation and verification are complete, set `ACS-72` to `Done`.
