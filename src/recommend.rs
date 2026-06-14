use anyhow::{bail, Context, Result};
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::fetch;
use crate::paths;
use crate::sites::{self, RecommendSource};

#[derive(Debug, PartialEq, Eq)]
struct LinkCandidate {
    title: String,
    url: String,
}

enum RecommendationTarget<'a> {
    AllSources,
    Source {
        site_name: &'static str,
        source: RecommendSource,
    },
    PageLinks {
        url: &'a str,
    },
}

const MAX_LIMIT: usize = 100;
const ALL_TARGET: &str = "all";
const USER_AGENT: &str = concat!("article-collector/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, PartialEq, Eq)]
pub struct RecommendationCollection {
    pub item_count: usize,
    pub source_count: usize,
    pub raw_path: PathBuf,
    pub translation_required: bool,
}

pub async fn collect_recommended(target: &str, limit: usize) -> Result<RecommendationCollection> {
    validate_limit(limit)?;

    let recommendation_target = resolve_recommendation_target(target)?;
    let translation_required = recommendation_target.translation_required();
    let items = match recommendation_target {
        RecommendationTarget::AllSources => collect_all_sources(limit).await?,
        RecommendationTarget::Source { site_name, source } => {
            collect_source(site_name, source, limit).await?
        }
        RecommendationTarget::PageLinks { url } => {
            collect_page_links(url, "generic-web", None, limit).await?
        }
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
    Ok(RecommendationCollection {
        item_count: items.len(),
        source_count: source_count_for_target(target)?,
        raw_path,
        translation_required,
    })
}

fn resolve_recommendation_target(target: &str) -> Result<RecommendationTarget<'_>> {
    if is_all_target(target) {
        if sites::recommendable_sites().is_empty() {
            bail!("No recommendation sources configured");
        }
        return Ok(RecommendationTarget::AllSources);
    }

    if let Some(site) = sites::site_by_name(target) {
        return site
            .recommend
            .map(|source| RecommendationTarget::Source {
                site_name: site.name,
                source,
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No recommendation source configured for site '{}'. Use one of: {}, {}",
                    site.name,
                    ALL_TARGET,
                    sites::recommendable_site_names().join(", ")
                )
            });
    }

    if fetch::validate_url(target).is_err() {
        bail!(
            "Unknown site or invalid URL: {target}. Use one of: {}, {}. Supported URL examples: {}",
            ALL_TARGET,
            sites::recommendable_site_names().join(", "),
            sites::supported_url_examples().join(", ")
        );
    }

    if let Some(source) = sites::recommend_source_for_url(target) {
        let site_name = sites::SITES
            .iter()
            .find(|site| site.recommend == Some(source))
            .map(|site| site.name)
            .unwrap_or("unknown");
        Ok(RecommendationTarget::Source { site_name, source })
    } else {
        Ok(RecommendationTarget::PageLinks { url: target })
    }
}

impl RecommendationTarget<'_> {
    fn translation_required(&self) -> bool {
        matches!(self, Self::AllSources)
    }
}

fn is_all_target(target: &str) -> bool {
    target.trim().eq_ignore_ascii_case(ALL_TARGET)
}

fn source_count_for_target(target: &str) -> Result<usize> {
    Ok(match resolve_recommendation_target(target)? {
        RecommendationTarget::AllSources => sites::recommendable_sites().len(),
        RecommendationTarget::Source { .. } | RecommendationTarget::PageLinks { .. } => 1,
    })
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
        .user_agent(USER_AGENT)
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

    let scan_limit = top_ids.len().min(limit.saturating_mul(10).max(20));
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

async fn collect_devto_articles(api_url: &str, limit: usize) -> Result<Vec<Value>> {
    let mut url = Url::parse(api_url).context("Invalid Dev.to articles API URL")?;
    url.query_pairs_mut()
        .append_pair("per_page", &limit.to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let articles: Vec<Value> = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("Failed to parse Dev.to articles response")?;

    Ok(articles
        .into_iter()
        .filter_map(|article| devto_article_to_recommendation(&article))
        .take(limit)
        .enumerate()
        .map(|(index, mut item)| {
            if let Some(object) = item.as_object_mut() {
                object.insert("rank".to_string(), json!(index + 1));
            }
            item
        })
        .collect())
}

fn devto_article_to_recommendation(data: &Value) -> Option<Value> {
    let url = data.get("url")?.as_str()?.to_string();
    if url.trim().is_empty() {
        return None;
    }

    let title = data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let author = data
        .get("user")
        .and_then(|user| user.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let description = data
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let tags = data
        .get("tag_list")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let content = format!(
        "Title: {title}\nURL: {url}\nAuthor: {author}\nTags: {tags}\nDescription: {description}"
    );

    Some(json!({
        "source": "devto",
        "title": title,
        "url": url,
        "author": author,
        "description": description,
        "tags": tags,
        "published_at": data.get("published_at").and_then(Value::as_str).unwrap_or(""),
        "reactions": data.get("public_reactions_count").and_then(Value::as_u64).unwrap_or(0),
        "comments": data.get("comments_count").and_then(Value::as_u64).unwrap_or(0),
        "content": content
    }))
}

async fn collect_all_sources(limit: usize) -> Result<Vec<Value>> {
    let recommendable_sites = sites::recommendable_sites();
    let mut items = Vec::new();

    for site in recommendable_sites {
        let source = site
            .recommend
            .context("recommendable site must have a recommend source")?;
        let mut site_items = collect_source(site.name, source, limit)
            .await
            .with_context(|| {
                format!(
                    "Failed to collect recommended articles for site '{}'",
                    site.name
                )
            })?;
        if site_items.is_empty() {
            bail!("No recommended articles found for site '{}'", site.name);
        }

        items.append(&mut site_items);
    }

    Ok(items)
}

async fn collect_source(
    site_name: &'static str,
    source: RecommendSource,
    limit: usize,
) -> Result<Vec<Value>> {
    let mut items = match source {
        RecommendSource::HackerNewsTopStories { api_url } => {
            collect_hackernews_topstories(api_url, limit).await?
        }
        RecommendSource::DevToArticles { api_url } => {
            collect_devto_articles(api_url, limit).await?
        }
    };

    for item in &mut items {
        if let Some(object) = item.as_object_mut() {
            object.insert("site".to_string(), json!(site_name));
        }
    }

    Ok(items)
}

async fn collect_page_links(
    url: &str,
    source_name: &str,
    site_name: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>> {
    let base = Url::parse(url).context("Invalid URL")?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(USER_AGENT)
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
                "source": source_name,
                "title": link.title,
                "url": link.url,
                "source_url": url,
                "content": content
            })
        })
        .map(|mut item| {
            if let (Some(site_name), Some(object)) = (site_name, item.as_object_mut()) {
                object.insert("site".to_string(), json!(site_name));
            }
            item
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
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::HackerNewsTopStories { .. },
        } = resolve_recommendation_target("hackernews").unwrap()
        else {
            panic!("hackernews should resolve to HN topstories");
        };
        assert_eq!(site_name, "hackernews");
    }

    /// 検証: HN URL も topstories 収集として扱う
    /// 理由: 既存の URL 入力フローとの互換性を保つ
    /// リスク: HN URL が generic link 抽出に落ち、上位記事を収集できない
    #[test]
    fn resolves_hackernews_url_as_topstories() {
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::HackerNewsTopStories { .. },
        } = resolve_recommendation_target("https://news.ycombinator.com/item?id=123").unwrap()
        else {
            panic!("HN URL should resolve to HN topstories");
        };
        assert_eq!(site_name, "hackernews");
    }

    /// 検証: Dev.to の site 名は Dev.to API 収集へ解決する
    /// 理由: site 名だけで Dev.to の上位記事取得を開始できるようにする
    /// リスク: Dev.to が汎用リンク抽出に落ち、記事以外のリンクが混ざる
    #[test]
    fn resolves_devto_site_name_to_articles_api() {
        let RecommendationTarget::Source {
            site_name,
            source: RecommendSource::DevToArticles { api_url },
        } = resolve_recommendation_target("devto").unwrap()
        else {
            panic!("devto should resolve to Dev.to articles");
        };
        assert_eq!(site_name, "devto");
        assert_eq!(api_url, "https://dev.to/api/articles?top=7");
    }

    /// 検証: all は registry 上の全 recommend source 収集として扱う
    /// 理由: 個別 site 名を列挙せずに一括収集と翻訳を開始できるようにする
    /// リスク: all が未知 site として拒否され、バッチ用途で使えない
    #[test]
    fn resolves_all_keyword_to_all_sources() {
        let target = resolve_recommendation_target("ALL").unwrap();
        assert!(matches!(target, RecommendationTarget::AllSources));
        assert!(target.translation_required());
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

    /// 検証: all の source 数は recommend 可能な site 数と一致する
    /// 理由: `--limit` は各 source ごとの上限として適用する
    /// リスク: 最初の site だけで上限を使い切り、後続 site から取得されない
    #[test]
    fn counts_all_sources_from_site_registry() {
        assert_eq!(
            source_count_for_target("all").unwrap(),
            sites::recommendable_sites().len()
        );
        assert_eq!(source_count_for_target("hackernews").unwrap(), 1);
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

    /// 検証: Dev.to API item から記事推薦項目へ変換する
    /// 理由: `recommend devto` と `recommend all` で Dev.to の記事 API レスポンスを raw.json に正規化する
    /// リスク: Dev.to から記事 URL を取得しても翻訳対象の content が空になる
    #[test]
    fn converts_devto_article_to_recommendation() {
        let item = json!({
            "title": "Example Dev.to story",
            "url": "https://dev.to/example/story",
            "description": "Short description",
            "tag_list": ["rust", "cli"],
            "user": {"name": "alice"},
            "published_at": "2026-06-14T00:00:00Z",
            "public_reactions_count": 10,
            "comments_count": 2
        });

        let recommendation = devto_article_to_recommendation(&item).unwrap();
        assert_eq!(recommendation["source"], "devto");
        assert_eq!(recommendation["title"], "Example Dev.to story");
        assert_eq!(recommendation["url"], "https://dev.to/example/story");
        assert_eq!(recommendation["author"], "alice");
        assert_eq!(recommendation["tags"], "rust, cli");
        assert_eq!(
            recommendation["content"],
            "Title: Example Dev.to story\nURL: https://dev.to/example/story\nAuthor: alice\nTags: rust, cli\nDescription: Short description"
        );
    }

    /// 検証: URL がない Dev.to item は推薦項目から除外する
    /// 理由: 翻訳後に参照できない記事を raw.json に入れない
    /// リスク: 空 URL の候補が保存され、後続の確認や保存処理が壊れる
    #[test]
    fn skips_devto_articles_without_url() {
        let item = json!({"title": "No URL"});
        assert!(devto_article_to_recommendation(&item).is_none());
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

    /// 検証: registry に登録された全 recommend source から実際に記事を取得できる
    /// 理由: `recommend all` は全 site の外部 API / ページ構造に依存するため、単体変換だけでは壊れた取得経路を検出できない
    /// リスク: ある site の source が壊れても all 実行まで気づけない
    #[tokio::test]
    async fn collects_recommendations_from_every_registered_site() {
        let items = collect_all_sources(1).await.unwrap();
        let collected_sites = items
            .iter()
            .filter_map(|item| item.get("site").and_then(Value::as_str))
            .collect::<HashSet<_>>();

        for site in sites::recommendable_sites() {
            assert!(
                collected_sites.contains(site.name),
                "missing recommendations from {}",
                site.name
            );
        }

        assert!(items.iter().all(|item| item
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| url.starts_with("https://") || url.starts_with("http://"))));
    }
}
