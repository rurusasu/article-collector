use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_FILE: &str = "article-collector.toml";

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ArticleCollectorConfig {
    pub recommend: RecommendConfig,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RecommendConfig {
    pub limit: Option<usize>,
    pub sources: Option<Vec<String>>,
    pub fetch_articles: bool,
    pub create_pr: bool,
    pub history_path: Option<PathBuf>,
    pub source: BTreeMap<String, RecommendSiteConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RecommendSiteConfig {
    pub enabled: Option<bool>,
    pub limit: Option<usize>,
    pub query: Option<String>,
}

pub fn load(path: Option<&Path>) -> Result<ArticleCollectorConfig> {
    let Some(path) = resolve_config_path(path) else {
        return Ok(ArticleCollectorConfig::default());
    };
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    parse_config(&raw).with_context(|| format!("Failed to parse config file {}", path.display()))
}

fn resolve_config_path(path: Option<&Path>) -> Option<PathBuf> {
    match path {
        Some(path) => Some(path.to_path_buf()),
        None => {
            let default_path = PathBuf::from(DEFAULT_CONFIG_FILE);
            default_path.exists().then_some(default_path)
        }
    }
}

fn parse_config(raw: &str) -> Result<ArticleCollectorConfig> {
    toml::from_str(raw).context("Invalid TOML config")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 検証: recommend source 別の arXiv query/limit を TOML から読める
    /// 理由: all 実行時の論文カテゴリをコード変更なしで調整したい
    /// リスク: config を追加しても arXiv 既定 query しか使えない
    #[test]
    fn parses_recommend_source_config() {
        let config = parse_config(
            r#"
            [recommend]
            sources = ["hackernews", "arxiv"]
            limit = 30

            [recommend.source.arxiv]
            limit = 10
            query = "cat:cs.IR OR cat:cs.SE"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.recommend.sources,
            Some(vec!["hackernews".to_string(), "arxiv".to_string()])
        );
        assert_eq!(config.recommend.limit, Some(30));
        assert_eq!(config.recommend.source["arxiv"].limit, Some(10));
        assert_eq!(
            config.recommend.source["arxiv"].query.as_deref(),
            Some("cat:cs.IR OR cat:cs.SE")
        );
    }

    /// 検証: recommend history DB path を TOML から読める
    /// 理由: cron や手動実行で同じ SQLite 履歴を明示的に共有したい
    /// リスク: outdir が変わるたびに重複排除が効かなくなる
    #[test]
    fn parses_recommend_history_path() {
        let config = parse_config(
            r#"
        [recommend]
        history_path = "D:/article-collector-data/recommend-history.sqlite"
        "#,
        )
        .unwrap();

        assert_eq!(
            config.recommend.history_path,
            Some(std::path::PathBuf::from(
                "D:/article-collector-data/recommend-history.sqlite"
            ))
        );
    }

    /// 検証: recommend article fetching の有効化を TOML から読める
    /// 理由: cron 実行では CLI option ではなく config file 側で取得モードを制御したい
    /// リスク: config に書いても通常の推薦一覧だけが出力され、記事本文取得に進まない
    #[test]
    fn parses_recommend_fetch_articles_switch() {
        let config = parse_config(
            r#"
        [recommend]
        fetch_articles = true
        "#,
        )
        .unwrap();

        assert!(config.recommend.fetch_articles);
    }

    /// 検証: recommend の PR 作成を TOML から opt-in できる
    /// 理由: cron 実行では CLI flag ではなく config file 側で PR 作成まで自動化したい
    /// リスク: 設定に書いても recommend が target repo への保存/PR 作成へ進まない
    #[test]
    fn parses_recommend_create_pr_switch() {
        let config = parse_config(
            r#"
        [recommend]
        create_pr = true
        "#,
        )
        .unwrap();

        assert!(config.recommend.create_pr);
    }

    /// 検証: 未知 key は config parse error にする
    /// 理由: typo した設定を無視すると cron の取得対象が意図せず既定値に戻る
    /// リスク: `qery` のような typo に気づかず、誤った推薦結果を使い続ける
    #[test]
    fn rejects_unknown_config_keys() {
        let error = parse_config(
            r#"
            [recommend.source.arxiv]
            qery = "cat:cs.IR"
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("Invalid TOML config"));
    }
}
