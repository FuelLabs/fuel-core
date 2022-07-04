use fuel_core::service::FuelService;

mod args;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // load configuration

    args::run_cli().await?;

    Ok(())
}
