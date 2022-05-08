use clap::Parser;
use fuel_core::service::FuelService;
use std::io;
use tracing::{info, trace};

mod args;

#[tokio::main]
async fn main() -> io::Result<()> {
    // load configuration
    let config = args::Opt::parse().exec()?;
    // log fuel-core version
    info!("Fuel Core version v{}", env!("CARGO_PKG_VERSION"));
    trace!("Initializing in TRACE mode.");
    // initialize the server
    let server = FuelService::new_node(config).await?;
    // pause the main task while service is running
    server.run().await;
    Ok(())
}
