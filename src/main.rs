mod config;
mod fetch;
mod paths;
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
        /// 推薦 URL の記事本文も取得して記事別 artifact を作成
        #[arg(long)]
        fetch_articles: bool,
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
            fetch_articles,
        } => {
            let app_config = config::load(config.as_deref())?;
            let collection = recommend::collect_recommended(
                target,
                limit,
                query.as_deref(),
                &app_config.recommend,
                fetch_articles,
            )
            .await?;
            if collection.translation_required {
                translate::translate(&collection.raw_path).await?;
            }
        }
    }

    Ok(())
}
