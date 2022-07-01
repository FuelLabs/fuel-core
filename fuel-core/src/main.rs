use clap::Parser;
use fuel_core::service::FuelService;
use tracing::{info, trace};

mod args;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // load configuration
    let opt = args::Opt::parse();
    let snapshot = opt.clone().snapshot;
    match snapshot {
        Some(args::Snapshot::Snapshot(_)) => {
            let snapshot = snapshot.unwrap().get_args().unwrap();
            args::dump_snapshot(
                snapshot.database_path,
                snapshot.chain_config.as_str().parse().unwrap(),
            )?;
        }
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
