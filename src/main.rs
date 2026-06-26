mod config;
mod discovery;
mod fetch;
mod paths;
mod pipeline;
mod recommend;
mod recommend_artifacts;
mod recommend_history;
mod save;
mod sites;
mod target_repos;
mod translate;
mod youtube;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "article-collector",
    version,
    about = "記事取得 → 翻訳 → 保存 → PR"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 記事取得 → 翻訳 → 保存 → PR（全工程）
    Collect {
        /// 取得する記事の URL
        url: String,
    },
    /// URL から記事を取得
    Fetch {
        /// 取得する記事の URL
        url: String,
    },
    /// 取得した記事を翻訳
    Translate {
        /// 入力 JSON ファイルパス
        input: Option<PathBuf>,
    },
    /// 翻訳記事を target repo の作業ブランチへ保存
    Save {
        /// 元記事の URL
        url: String,
    },
    /// 保存済み Markdown を commit / push して PR 作成
    Pr {
        /// target repo からの相対 path、または target repo 配下の絶対 path
        path: PathBuf,
    },
    /// 推薦記事/関連リンクをまとめて取得
    Recommend {
        /// 推薦記事を探す site 名、all、または起点 URL
        target: String,
        /// 収集する最大件数
        #[arg(short, long)]
        limit: Option<usize>,
        /// arXiv など query 対応 source の検索条件
        #[arg(long)]
        query: Option<String>,
        /// article-collector TOML config のパス
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
    /// recommend history を管理
    History {
        #[command(subcommand)]
        command: HistoryCommands,
    },
}

#[derive(Subcommand)]
enum HistoryCommands {
    /// SQLite recommend history を clear
    Clear {
        /// article-collector TOML config のパス
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collect { ref url } => {
            fetch::fetch_url(url).await?;
            if translate::translate(&paths::raw_json_path()).await?
                == translate::TranslateOutcome::Translated
            {
                let prepared = target_repos::prepare_article_branch()?;
                let saved = save::save_article_to_target(&prepared.target_dir, url)?;
                target_repos::create_pr_for_path(&saved.path)?;
            }
        }
        Commands::Fetch { ref url } => {
            fetch::fetch_url(url).await?;
        }
        Commands::Translate { ref input } => {
            let input = input.clone().unwrap_or_else(paths::raw_json_path);
            translate::translate(&input).await?;
        }
        Commands::Save { ref url } => {
            let prepared = target_repos::prepare_article_branch()?;
            let saved = save::save_article_to_target(&prepared.target_dir, url)?;
            eprintln!(
                "Saved article on branch {}: {}",
                prepared.branch,
                saved.path.display()
            );
        }
        Commands::Pr { ref path } => {
            target_repos::create_pr_for_path(path)?;
        }
        Commands::Recommend {
            ref target,
            limit,
            ref query,
            ref config,
        } => {
            let app_config = config::load(config.as_deref())?;
            let collection = recommend::collect_recommended(
                target,
                limit,
                query.as_deref(),
                &app_config.recommend,
            )
            .await?;
            if collection.translation_required {
                translate::translate(&collection.raw_path).await?;
            }
            if app_config.recommend.create_pr {
                ensure_recommend_pr_has_articles(&collection)?;
                let prepared = target_repos::prepare_article_branch()?;
                let saved_articles = save::save_recommended_articles_to_target(
                    &prepared.target_dir,
                    &collection.translated_articles,
                )?;
                let saved_paths = saved_articles
                    .iter()
                    .map(|article| article.path.clone())
                    .collect::<Vec<_>>();
                let pr_title = recommend_pr_title(target, saved_articles.len());
                let pr_body = recommend_pr_body(&saved_articles);
                target_repos::create_pr_for_paths(&saved_paths, &pr_title, &pr_title, &pr_body)?;
            }
        }
        Commands::History {
            command: HistoryCommands::Clear { ref config },
        } => {
            let app_config = config::load(config.as_deref())?;
            let history_path = recommend::history_path_for_config(&app_config.recommend)?;
            let mut history = recommend_history::RecommendationHistory::open(&history_path)?;
            let cleared = history.clear_seen_items()?;
            eprintln!(
                "Cleared {cleared} recommend history item(s) from {}",
                history_path.display()
            );
        }
    }

    Ok(())
}

fn recommend_pr_title(target: &str, count: usize) -> String {
    let noun = if count == 1 { "article" } else { "articles" };
    format!("recommend: collect {count} {noun} from {target}")
}

fn recommend_pr_body(saved_articles: &[save::SavedArticle]) -> String {
    let mut lines = vec!["## Recommended Articles".to_string(), String::new()];
    for article in saved_articles {
        lines.push(format!(
            "- `{}`",
            article.repo_relative_path.to_string_lossy()
        ));
    }
    lines.join("\n")
}

fn ensure_recommend_pr_has_articles(
    collection: &recommend::RecommendationCollection,
) -> Result<()> {
    if collection.translated_articles.is_empty() {
        anyhow::bail!("No translated recommended articles available for PR creation");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_pr_body_lists_all_saved_article_paths() {
        let saved_articles = vec![
            save::SavedArticle {
                path: PathBuf::from("D:/target/articles/web/first.md"),
                repo_relative_path: PathBuf::from("articles/web/first.md"),
                title: "First".to_string(),
            },
            save::SavedArticle {
                path: PathBuf::from("D:/target/articles/paper/second.md"),
                repo_relative_path: PathBuf::from("articles/paper/second.md"),
                title: "Second".to_string(),
            },
        ];

        let body = recommend_pr_body(&saved_articles);

        assert!(body.contains("## Recommended Articles"));
        assert!(body.contains("- `articles/web/first.md`"));
        assert!(body.contains("- `articles/paper/second.md`"));
    }

    #[test]
    fn recommend_pr_title_uses_singular_and_plural_nouns() {
        assert_eq!(
            recommend_pr_title("hackernews", 1),
            "recommend: collect 1 article from hackernews"
        );
        assert_eq!(
            recommend_pr_title("all", 2),
            "recommend: collect 2 articles from all"
        );
    }

    #[test]
    fn recommend_pr_requires_translated_articles_before_target_repo_side_effects() {
        let collection = recommend::RecommendationCollection {
            item_count: 0,
            source_count: 1,
            raw_path: PathBuf::from("raw.json"),
            translation_required: false,
            translated_articles: Vec::new(),
        };

        let error = ensure_recommend_pr_has_articles(&collection).unwrap_err();

        assert_eq!(
            error.to_string(),
            "No translated recommended articles available for PR creation"
        );
    }
}
