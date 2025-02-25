use clap::Parser;
use fuel_core_srs::{
    app::App,
    cli::Args,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let app = App::new(&args).await?;

    app.run(&args).await
}
