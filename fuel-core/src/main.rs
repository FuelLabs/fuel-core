use clap::Parser;
use fuel_core::service::FuelService;
use tracing::{info, trace};

mod args;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // load configuration
    let opt = args::Opt::parse();
    match opt._snapshot {
        Some(args::SnapshotCommand::Snapshot) => args::dump_snapshot()?,
        _ => {
            let config = opt.get_config()?;
            // log fuel-core version
            info!("Fuel Core version v{}", env!("CARGO_PKG_VERSION"));
            trace!("Initializing in TRACE mode.");
            // initialize the server
            let server = FuelService::new_node(config).await?;
            // pause the main task while service is running
            server.run().await;
        }
    }
    Ok(())
}
