use anyhow::{bail, Context, Result};
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;

use crate::fetch;
use crate::paths;
use crate::sites::{self, RecommendSource};

#[derive(Debug, PartialEq, Eq)]
struct LinkCandidate {
    title: String,
    url: String,
}

enum RecommendationTarget<'a> {
    Source(RecommendSource),
    PageLinks { url: &'a str },
}

const MAX_LIMIT: usize = 100;

pub async fn collect_recommended(target: &str, limit: usize) -> Result<()> {
    validate_limit(limit)?;

    let recommendation_target = resolve_recommendation_target(target)?;
    let items = match recommendation_target {
        RecommendationTarget::Source(RecommendSource::HackerNewsTopStories { api_url }) => {
            collect_hackernews_topstories(api_url, limit).await?
        }
        RecommendationTarget::Source(RecommendSource::PageLinks { url }) => {
            collect_generic_page_links(url, limit).await?
        }
        RecommendationTarget::PageLinks { url } => collect_generic_page_links(url, limit).await?,
    };

    if items.is_empty() {
        bail!("No recommended articles found for {target}");
    }

    let outdir = paths::outdir();
    fs::create_dir_all(&outdir)?;
    let raw_path = paths::raw_json_path();
    fs::write(&raw_path, serde_json::to_string_pretty(&items)?)?;

    eprintln!(
        "Recommended articles collected: {} item(s) -> {}",
        items.len(),
        raw_path.display()
    );
    Ok(())
}

fn resolve_recommendation_target(target: &str) -> Result<RecommendationTarget<'_>> {
    if let Some(site) = sites::site_by_name(target) {
        return site
            .recommend
            .map(RecommendationTarget::Source)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No recommendation source configured for site '{}'. Use one of: {}",
                    site.name,
                    sites::recommendable_site_names().join(", ")
                )
            });
    }

    if fetch::validate_url(target).is_err() {
        bail!(
            "Unknown site or invalid URL: {target}. Use one of: {}. Supported URL examples: {}",
            sites::recommendable_site_names().join(", "),
            sites::supported_url_examples().join(", ")
        );
    }

    if let Some(source) = sites::recommend_source_for_url(target) {
        Ok(RecommendationTarget::Source(source))
    } else {
        Ok(RecommendationTarget::PageLinks { url: target })
    }
}

fn validate_limit(limit: usize) -> Result<()> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        bail!("limit must be between 1 and {MAX_LIMIT}");
    }
    Ok(())
}

async fn collect_hackernews_topstories(api_url: &str, limit: usize) -> Result<Vec<Value>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let top_ids: Vec<u64> = client
        .get(api_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("Failed to parse HN topstories response")?;

    if top_ids.is_empty() {
        bail!("Hacker News topstories returned no IDs");
    }

    let scan_limit = top_ids.len().min(limit.saturating_mul(4).max(limit));
    let mut items = Vec::new();

    for id in top_ids.into_iter().take(scan_limit) {
        if items.len() >= limit {
            break;
        }

        let api_url = format!("https://hacker-news.firebaseio.com/v0/item/{id}.json");
        let response = client.get(&api_url).send().await;
        let data = match response {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(data) => data,
                    Err(err) => {
                        eprintln!("WARN: Failed to parse HN item {id}: {err}");
                        continue;
                    }
                },
                Err(err) => {
                    eprintln!("WARN: Failed to fetch HN item {id}: {err}");
                    continue;
                }
            },
            Err(err) => {
                eprintln!("WARN: Failed to fetch HN item {id}: {err}");
                continue;
            }
        };

        if let Some(item) = hackernews_item_to_recommendation(&data, items.len() + 1) {
            items.push(item);
        }
    }

    Ok(items)
}

fn hackernews_item_to_recommendation(data: &Value, rank: usize) -> Option<Value> {
    let url = data.get("url")?.as_str()?.to_string();
    if url.trim().is_empty() {
        return None;
    }

    let id = data.get("id").and_then(Value::as_u64).unwrap_or(0);
    let title = data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let author = data.get("by").and_then(Value::as_str).unwrap_or("");
    let score = data.get("score").and_then(Value::as_u64).unwrap_or(0);
    let comments = data.get("descendants").and_then(Value::as_u64).unwrap_or(0);
    let hn_url = format!("https://news.ycombinator.com/item?id={id}");
    let content = format!(
        "Title: {title}\nURL: {url}\nHacker News: {hn_url}\nScore: {score}\nComments: {comments}"
    );

    Some(json!({
        "rank": rank,
        "source": "hackernews",
        "id": id,
        "title": title,
        "url": url,
        "hn_url": hn_url,
        "author": author,
        "score": score,
        "comments": comments,
        "time": data.get("time").and_then(Value::as_u64).unwrap_or(0),
        "type": data.get("type").and_then(Value::as_str).unwrap_or("story"),
        "content": content
    }))
}

async fn collect_generic_page_links(url: &str, limit: usize) -> Result<Vec<Value>> {
    let base = Url::parse(url).context("Invalid URL")?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;
    let html = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let links = extract_page_links(&base, &html, limit);
    Ok(links
        .into_iter()
        .enumerate()
        .map(|(index, link)| {
            let rank = index + 1;
            let content = format!(
                "Title: {}\nURL: {}\nSource page: {}",
                link.title, link.url, url
            );
            json!({
                "rank": rank,
                "source": "generic-web",
                "title": link.title,
                "url": link.url,
                "source_url": url,
                "content": content
            })
        })
        .collect())
}

fn extract_page_links(base: &Url, html: &str, limit: usize) -> Vec<LinkCandidate> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").expect("valid selector");
    let mut seen = HashSet::new();
    let mut base_without_fragment = base.clone();
    base_without_fragment.set_fragment(None);
    let base_url = base_without_fragment.to_string();
    let mut links = Vec::new();

    for element in document.select(&selector) {
        if links.len() >= limit {
            break;
        }

        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Ok(mut link_url) = base.join(href) else {
            continue;
        };
        if link_url.scheme() != "http" && link_url.scheme() != "https" {
            continue;
        }
        link_url.set_fragment(None);

        let url = link_url.to_string();
        if url == base_url || !seen.insert(url.clone()) {
            continue;
        }

        let title = normalize_link_text(element.text().collect::<Vec<_>>().join(" "));
        links.push(LinkCandidate {
            title: if title.is_empty() { url.clone() } else { title },
            url,
        });
    }

    links
}

fn normalize_link_text(text: String) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 検証: Hacker News の site 名は topstories 収集として扱う
    /// 理由: 長い HN item URL を毎回入力せずに recommend を実行できるようにする
    /// リスク: site 名入力が使えずワンコマンド性が落ちる
    #[test]
    fn resolves_hackernews_site_name_as_topstories() {
        let RecommendationTarget::Source(RecommendSource::HackerNewsTopStories { .. }) =
            resolve_recommendation_target("hackernews").unwrap()
        else {
            panic!("hackernews should resolve to HN topstories");
        };
    }

    /// 検証: HN URL も topstories 収集として扱う
    /// 理由: 既存の URL 入力フローとの互換性を保つ
    /// リスク: HN URL が generic link 抽出に落ち、上位記事を収集できない
    #[test]
    fn resolves_hackernews_url_as_topstories() {
        let RecommendationTarget::Source(RecommendSource::HackerNewsTopStories { .. }) =
            resolve_recommendation_target("https://news.ycombinator.com/item?id=123").unwrap()
        else {
            panic!("HN URL should resolve to HN topstories");
        };
    }

    /// 検証: PageLinks source を持つ site 名は既定 URL へ解決する
    /// 理由: site 名だけで長い URL を打たずに recommend を開始できるようにする
    /// リスク: registry に recommend URL を持っても CLI から利用できない
    #[test]
    fn resolves_page_link_site_name_to_default_url() {
        let RecommendationTarget::Source(RecommendSource::PageLinks { url }) =
            resolve_recommendation_target("devto").unwrap()
        else {
            panic!("devto should resolve to page links");
        };
        assert_eq!(url, "https://dev.to/");
    }

    /// 検証: HN 以外の http(s) URL は generic page link 抽出として扱う
    /// 理由: まずはページ内リンクをワンコマンドで収集する fallback を提供する
    /// リスク: 未対応サイトで何も収集できない
    #[test]
    fn resolves_other_urls_as_generic_links() {
        let RecommendationTarget::PageLinks { url } =
            resolve_recommendation_target("https://example.com/article").unwrap()
        else {
            panic!("generic URL should resolve to page links");
        };
        assert_eq!(url, "https://example.com/article");
    }

    /// 検証: recommend source 未設定の site 名は明示的に拒否する
    /// 理由: site 名だけで無根拠な collection URL を推測しない
    /// リスク: YouTube 等で意図しないページからリンクを収集する
    #[test]
    fn rejects_site_names_without_recommend_source() {
        assert!(resolve_recommendation_target("youtube").is_err());
    }

    /// 検証: 未知の site 名は URL ではなく入力エラーとして扱う
    /// 理由: typo を generic URL として扱わない
    /// リスク: 誤入力が分かりにくいネットワークエラーになる
    #[test]
    fn rejects_unknown_site_names() {
        assert!(resolve_recommendation_target("unknown-site").is_err());
    }

    /// 検証: limit は 1 以上 100 以下だけ許可する
    /// 理由: 誤入力で大量の外部リクエストを発生させない
    /// リスク: limit=0 や巨大値で空出力または過剰通信になる
    #[test]
    fn validates_limit_range() {
        assert!(validate_limit(1).is_ok());
        assert!(validate_limit(30).is_ok());
        assert!(validate_limit(100).is_ok());
        assert!(validate_limit(0).is_err());
        assert!(validate_limit(101).is_err());
    }

    /// 検証: HN item からリンク付き story を推薦項目へ変換する
    /// 理由: topstories API の item JSON を raw.json の配列要素に正規化する
    /// リスク: URL 付き story が欠落し、推薦一覧が空になる
    #[test]
    fn converts_hackernews_item_to_recommendation() {
        let item = json!({
            "id": 42,
            "title": "Example story",
            "url": "https://example.com/story",
            "by": "alice",
            "score": 123,
            "descendants": 45,
            "time": 1000,
            "type": "story"
        });

        let recommendation = hackernews_item_to_recommendation(&item, 1).unwrap();
        assert_eq!(recommendation["rank"], 1);
        assert_eq!(recommendation["source"], "hackernews");
        assert_eq!(recommendation["title"], "Example story");
        assert_eq!(recommendation["url"], "https://example.com/story");
        assert_eq!(
            recommendation["hn_url"],
            "https://news.ycombinator.com/item?id=42"
        );
    }

    /// 検証: URL がない HN item はページ推薦から除外する
    /// 理由: Ask HN 等は推薦先ページを持たない
    /// リスク: 内部 discussion だけが上位ページとして混ざる
    #[test]
    fn skips_hackernews_items_without_url() {
        let item = json!({"id": 42, "title": "Ask HN"});
        assert!(hackernews_item_to_recommendation(&item, 1).is_none());
    }

    /// 検証: generic HTML から http(s) リンクを順序どおり抽出する
    /// 理由: 未対応サイトでもページ内の推薦リンク候補を raw.json にできる
    /// リスク: 相対 URL や重複リンクが壊れたまま出力される
    #[test]
    fn extracts_generic_page_links() {
        let base = Url::parse("https://example.com/articles/root#section").unwrap();
        let html = r#"
            <a href="/first"> First article </a>
            <a href="https://example.org/second#comments">Second article</a>
            <a href="/first">Duplicate first article</a>
            <a href="mailto:test@example.com">Mail</a>
            <a href="https://example.com/articles/root#other">Self</a>
        "#;

        let links = extract_page_links(&base, html, 10);
        assert_eq!(
            links,
            vec![
                LinkCandidate {
                    title: "First article".to_string(),
                    url: "https://example.com/first".to_string()
                },
                LinkCandidate {
                    title: "Second article".to_string(),
                    url: "https://example.org/second".to_string()
                }
            ]
        );
    }

    /// 検証: generic link 抽出は limit で停止する
    /// 理由: 指定件数以上の推薦候補を処理しない
    /// リスク: 大きなページで不要なリンクを大量に出力する
    #[test]
    fn generic_page_links_respect_limit() {
        let base = Url::parse("https://example.com/").unwrap();
        let html = r#"
            <a href="/a">A</a>
            <a href="/b">B</a>
            <a href="/c">C</a>
        "#;

        let links = extract_page_links(&base, html, 2);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].url, "https://example.com/a");
        assert_eq!(links[1].url, "https://example.com/b");
    }
}
