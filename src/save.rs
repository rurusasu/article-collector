use anyhow::{bail, Context, Result};
use chrono::Local;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const OUTDIR: &str = "/tmp/collect";

pub fn save_and_pr(url: &str) -> Result<()> {
    let target_repo = std::env::var("TARGET_REPO").context("TARGET_REPO env var required")?;
    let target_dir = std::env::var("TARGET_DIR").unwrap_or_else(|_| "/tmp/target-repo".to_string());
    let save_path_template =
        std::env::var("SAVE_PATH_TEMPLATE").unwrap_or_else(|_| "articles/${TYPE}/".to_string());
    let auto_merge = std::env::var("AUTO_MERGE").unwrap_or_else(|_| "true".to_string());

    let now = Local::now().format("%Y-%m-%d").to_string();
    let branch = format!("collect/{}", Local::now().format("%Y-%m-%d-%H%M%S"));

    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("Invalid URL: {url}");
    }

    let article_type = determine_type(url);

    // Extract title from raw.json
    let raw_path = Path::new(OUTDIR).join("raw.json");
    let raw = fs::read_to_string(&raw_path).context("Failed to read raw.json")?;
    let data: Value = serde_json::from_str(&raw)?;

    let title = if let Some(arr) = data.as_array() {
        arr.first()
            .and_then(|item| {
                item.get("title").and_then(|t| t.as_str()).or_else(|| {
                    item.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| &s[..s.len().min(80)])
                })
            })
            .unwrap_or("untitled")
    } else {
        data.get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("untitled")
    };

    let title = sanitize_title(title);
    let slug = title_to_slug(&title);

    let filename = format!("{now}_{slug}.md");
    let save_path = save_path_template.replace("${TYPE}", &article_type);
    let dest_dir = PathBuf::from(&target_dir).join(&save_path);

    // Clone or update target repo
    let target_path = Path::new(&target_dir);
    if target_path.join(".git").exists() {
        run_git(&target_dir, &["checkout", "main"])?;
        run_git(&target_dir, &["pull", "origin", "main"])?;
    } else {
        run_cmd("gh", &["repo", "clone", &target_repo, &target_dir])?;
    }

    // Create branch
    run_git(&target_dir, &["checkout", "-b", &branch])?;

    // Create output file with frontmatter
    fs::create_dir_all(&dest_dir)?;
    let dest_file = dest_dir.join(&filename);

    let frontmatter = format!(
        "---\ntitle: \"{title}\"\ntype: {article_type}\nurl: \"{url}\"\ncreated: {now}\ntags: []\n---\n\n"
    );

    let translated_path = Path::new(OUTDIR).join("translated.md");
    let translated = fs::read_to_string(&translated_path).unwrap_or_default();

    // Validate
    if translated.is_empty() || translated.trim() == "null" {
        bail!("Translation result is empty or null, aborting");
    }

    let mut content = format!("{frontmatter}{translated}");

    // Append embedded translated articles
    if let Ok(entries) = fs::read_dir(OUTDIR) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("embedded_") && name.ends_with("_translated.md") {
                let emb = fs::read_to_string(entry.path()).unwrap_or_default();
                content.push_str("\n\n---\n\n## 関連記事\n\n");
                content.push_str(&emb);
            }
        }
    }

    fs::write(&dest_file, &content)?;

    // Commit + PR
    let rel_path = format!("{save_path}{filename}");
    run_git(&target_dir, &["add", &rel_path])?;
    run_git(&target_dir, &["commit", "-m", &format!("collect: {title}")])?;
    run_git(&target_dir, &["push", "-u", "origin", &branch])?;

    let pr_body = format!("## Collected Article\n\n- `{rel_path}` — {title}\n\nSource: {url}");
    run_cmd_in(
        &target_dir,
        "gh",
        &[
            "pr",
            "create",
            "--title",
            &format!("collect: {now} {title}"),
            "--body",
            &pr_body,
        ],
    )?;

    if auto_merge == "true" {
        run_cmd_in(&target_dir, "gh", &["pr", "merge", "--merge"])?;
    }

    // Return to main
    run_git(&target_dir, &["checkout", "main"])?;
    run_git(&target_dir, &["pull", "origin", "main"])?;

    eprintln!("Done: {}", dest_file.display());
    Ok(())
}

pub fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .take(200)
        .collect::<String>()
        .replace('"', "\\\"")
}

pub fn title_to_slug(title: &str) -> String {
    let slug_re = Regex::new(r"[^a-zA-Z0-9]").unwrap();
    let multi_dash = Regex::new(r"-{2,}").unwrap();
    let slug = slug_re.replace_all(title, "-").to_string();
    let slug = multi_dash.replace_all(&slug, "-").to_string();
    let slug = slug.trim_matches('-').to_lowercase();
    slug.chars().take(60).collect()
}

pub fn determine_type(url: &str) -> String {
    if url.contains("x.com/") || url.contains("twitter.com/") {
        "x".to_string()
    } else if url.contains("youtube.com/") || url.contains("youtu.be/") {
        "youtube".to_string()
    } else if url.contains("arxiv.org/")
        || url.contains("doi.org/")
        || url.contains("openreview.net/")
    {
        "paper".to_string()
    } else {
        "web".to_string()
    }
}

fn run_git(dir: &str, args: &[&str]) -> Result<()> {
    run_cmd_in(dir, "git", args)
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).args(args).status()?;
    if !status.success() {
        bail!("{cmd} {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn run_cmd_in(dir: &str, cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).current_dir(dir).args(args).status()?;
    if !status.success() {
        bail!("{cmd} {} failed with {status}", args.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── URL validation ──

    #[test]
    fn rejects_invalid_url() {
        // save_and_pr checks TARGET_REPO first, so test the URL validation directly
        use crate::fetch::validate_url;
        assert!(validate_url("not-a-url").is_err());
    }

    // ── type detection ──

    #[test]
    fn type_x_com() {
        assert_eq!(determine_type("https://x.com/user/status/123"), "x");
    }

    #[test]
    fn type_twitter_com() {
        assert_eq!(determine_type("https://twitter.com/user/status/123"), "x");
    }

    #[test]
    fn type_youtube_com() {
        assert_eq!(
            determine_type("https://www.youtube.com/watch?v=abc"),
            "youtube"
        );
    }

    #[test]
    fn type_youtu_be() {
        assert_eq!(determine_type("https://youtu.be/abc123"), "youtube");
    }

    #[test]
    fn type_arxiv() {
        assert_eq!(determine_type("https://arxiv.org/abs/2301.12345"), "paper");
    }

    #[test]
    fn type_doi() {
        assert_eq!(determine_type("https://doi.org/10.1234/example"), "paper");
    }

    #[test]
    fn type_openreview() {
        assert_eq!(
            determine_type("https://openreview.net/forum?id=abc"),
            "paper"
        );
    }

    #[test]
    fn type_generic() {
        assert_eq!(determine_type("https://example.com/article"), "web");
    }

    // ── title sanitization ──

    #[test]
    fn sanitize_strips_newlines() {
        assert_eq!(sanitize_title("Line One\nLine Two"), "Line OneLine Two");
    }

    #[test]
    fn sanitize_escapes_double_quotes() {
        assert_eq!(
            sanitize_title("Title \"with\" quotes"),
            "Title \\\"with\\\" quotes"
        );
    }

    #[test]
    fn sanitize_strips_carriage_returns() {
        assert_eq!(sanitize_title("Title\r\nWith CR"), "TitleWith CR");
    }

    #[test]
    fn sanitize_truncates_to_200_chars() {
        let long: String = "-".repeat(300);
        let result = sanitize_title(&long);
        assert_eq!(result.len(), 200);
    }

    #[test]
    fn sanitize_handles_mixed_special_chars() {
        assert_eq!(
            sanitize_title("He said \"hello\"\nand left\r"),
            "He said \\\"hello\\\"and left"
        );
    }

    // ── slug generation ──

    #[test]
    fn slug_lowercases() {
        assert_eq!(title_to_slug("Hello World"), "hello-world");
    }

    #[test]
    fn slug_replaces_special_chars() {
        assert_eq!(title_to_slug("foo@bar#baz"), "foo-bar-baz");
    }

    #[test]
    fn slug_collapses_multiple_dashes() {
        assert_eq!(title_to_slug("foo---bar"), "foo-bar");
    }

    #[test]
    fn slug_trims_leading_trailing_dashes() {
        assert_eq!(title_to_slug("-hello-"), "hello");
    }

    #[test]
    fn slug_truncates_to_60_chars() {
        let long: String = "a".repeat(100);
        let result = title_to_slug(&long);
        assert_eq!(result.len(), 60);
    }
}
