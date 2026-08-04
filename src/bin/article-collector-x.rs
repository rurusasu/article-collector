#[tokio::main]
async fn main() -> anyhow::Result<()> {
    article_collector::x_connector::stdio::run_stdio().await
}
