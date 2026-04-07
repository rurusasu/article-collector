mod fetch;
mod save;
mod translate;
mod youtube;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "article-collector", about = "記事取得 → 翻訳 → 保存 → PR")]
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
        #[arg(default_value = "/tmp/collect/raw.json")]
        input: PathBuf,
    },
    /// 翻訳記事を保存して PR 作成
    SaveAndPr {
        /// 元記事の URL
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collect { ref url } => {
            fetch::fetch_url(url).await?;
            translate::translate(&PathBuf::from("/tmp/collect/raw.json")).await?;
            save::save_and_pr(url)?;
        }
        Commands::Fetch { ref url } => {
            fetch::fetch_url(url).await?;
        }
        Commands::Translate { ref input } => {
            translate::translate(input).await?;
        }
        Commands::SaveAndPr { ref url } => {
            save::save_and_pr(url)?;
        }
    }

    Ok(())
}
