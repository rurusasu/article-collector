mod cleanup;
mod config;
mod discovery;
mod fetch;
mod logging;
mod paths;
mod pipeline;
mod plugins;
mod recommend;
mod recommend_artifacts;
mod recommend_history;
mod save;
mod sites;
mod target_repos;
mod translate;
mod youtube;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use url::Url;

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
    /// 記事を収集する
    Article {
        #[command(subcommand)]
        command: ArticleCommands,
    },
    /// 推薦履歴を管理する
    History {
        #[command(subcommand)]
        command: HistoryCommands,
    },
    /// Install and manage optional source plugins
    Plugin {
        #[command(subcommand)]
        command: plugins::commands::PluginCommands,
    },
}

#[derive(Subcommand)]
enum ArticleCommands {
    /// URL から記事を取得し、指定した工程まで進める
    Collect {
        /// 記事または探索の起点となる HTTP(S) URL
        #[arg(value_parser = parse_http_url, value_name = "URL")]
        url: Url,
        /// 起点 URL から記事候補を探索する
        #[arg(long)]
        discover: bool,
        /// 到達する工程。省略時は記事本文を保存し、--discover 指定時は候補一覧を保存する
        #[arg(long, value_enum)]
        through: Option<CollectThrough>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum CollectThrough {
    /// 記事本文を取得して JSON に保存する
    Raw,
    /// 記事を取得・翻訳して Markdown を作成する
    Translate,
    /// 翻訳済み記事をリポジトリに配置して PR を作成する（AUTO_MERGE 有効時はマージ）
    Pr,
}

#[derive(Subcommand)]
enum HistoryCommands {
    /// 推薦履歴を削除する
    Clear {
        /// 設定された一時・出力ディレクトリも配下のファイルごと削除する
        #[arg(long)]
        include_artifacts: bool,
        /// article-collector TOML config のパス
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
}

fn parse_http_url(input: &str) -> Result<Url, String> {
    let url = Url::parse(input).map_err(|error| error.to_string())?;
    if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
        return Err("HTTP(S) URL を指定してください".to_string());
    }
    Ok(url)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Article {
            command:
                ArticleCommands::Collect {
                    url,
                    discover,
                    through,
                },
        } => {
            run_collect(url.as_str(), discover, through).await?;
        }
        Commands::History {
            command:
                HistoryCommands::Clear {
                    include_artifacts,
                    ref config,
                },
        } => {
            let app_config = config::load(config.as_deref())?;
            let history_path = recommend::history_path_for_config(&app_config.recommend)?;
            let cleared = {
                let mut history = recommend_history::RecommendationHistory::open(&history_path)?;
                history.clear_seen_items()?
            };
            eprintln!(
                "Cleared {cleared} recommend history item(s) from {}",
                history_path.display()
            );
            if include_artifacts {
                let cleanup_summary = cleanup::clear_all_artifacts()?;
                if let Some(path) = cleanup_summary.removed_temp_dir {
                    eprintln!(
                        "Removed {} directory {}",
                        paths::TEMP_DIR_ENV,
                        path.display()
                    );
                }
                if let Some(path) = cleanup_summary.removed_output_dir {
                    eprintln!(
                        "Removed {} directory {}",
                        paths::OUTPUT_DIR_ENV,
                        path.display()
                    );
                }
            }
        }
        Commands::Plugin { ref command } => {
            let context = plugins::commands::production_context()?;
            for line in plugins::commands::execute(&context, command).await? {
                match command {
                    plugins::commands::PluginCommands::List => println!("{line}"),
                    _ => eprintln!("{line}"),
                }
            }
        }
    }

    Ok(())
}

async fn run_collect(url: &str, discover: bool, through: Option<CollectThrough>) -> Result<()> {
    if discover {
        return run_discovery(url, through).await;
    }

    if matches!(through, None | Some(CollectThrough::Raw)) {
        return fetch::fetch_url(url).await;
    }

    let agent = translate::require_acp_agent()?;
    fetch::fetch_url(url).await?;
    translate::translate(&paths::raw_json_path(), agent).await?;
    if through == Some(CollectThrough::Pr) {
        let target_dir = target_repos::prepare_article_branch()?;
        let saved = save::save_article_to_target(&target_dir, url)?;
        target_repos::create_pr_for_path(&saved.path)?;
    }
    Ok(())
}

async fn run_discovery(url: &str, through: Option<CollectThrough>) -> Result<()> {
    let output = match through {
        None => recommend::RecommendationOutput::Candidates,
        Some(CollectThrough::Raw) => recommend::RecommendationOutput::Raw,
        Some(CollectThrough::Translate | CollectThrough::Pr) => {
            translate::require_acp_agent()?;
            recommend::RecommendationOutput::Translated
        }
    };
    let app_config = config::load(None)?;
    let logger = logging::stderr_logger().new(slog::o!(
        "command" => "recommend".to_string(),
        "target" => url.to_owned(),
    ));
    let collection =
        recommend::collect_recommended_with_logger(url, &app_config.recommend, &logger, output)
            .await?;
    if through == Some(CollectThrough::Pr) {
        let target_dir = target_repos::prepare_article_branch()?;
        let saved_articles = save::save_recommended_articles_to_target(
            &target_dir,
            &collection.translated_articles,
        )?;
        let saved_paths = saved_articles
            .iter()
            .map(|article| article.path.clone())
            .collect::<Vec<_>>();
        let pr_title = recommend_pr_title(url, saved_articles.len());
        let pr_body = recommend_pr_body(&saved_articles);
        target_repos::create_pr_for_paths(&saved_paths, &pr_title, &pr_title, &pr_body)?;
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
        let seed = "https://example.com/articles";
        assert_eq!(
            recommend_pr_title(seed, 1),
            "recommend: collect 1 article from https://example.com/articles"
        );
        assert_eq!(
            recommend_pr_title(seed, 2),
            "recommend: collect 2 articles from https://example.com/articles"
        );
    }
}
