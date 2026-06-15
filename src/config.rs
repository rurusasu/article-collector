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
    pub source: BTreeMap<String, RecommendSourceConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RecommendSourceConfig {
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
