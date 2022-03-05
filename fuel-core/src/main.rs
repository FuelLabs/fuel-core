use clap::Parser;
use fuel_core::service::FuelService;
use std::io;
use tracing::trace;

mod args;

#[tokio::main]
async fn main() -> io::Result<()> {
    trace!("Initializing in TRACE mode.");
    // load configuration
    let config = args::Opt::parse().exec()?;
    // initialize the server
    let server = FuelService::new_node(config).await?;
    // pause the main task while service is running
    server.run().await;
    Ok(())
}
