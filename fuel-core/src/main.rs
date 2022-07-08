use fuel_core::service::FuelService;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run_cli().await
}
