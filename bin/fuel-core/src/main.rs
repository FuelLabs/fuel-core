#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(test)]
use rand as _;
#[cfg(test)]
use serial_test as _;

// Use Jemalloc for main binary
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use fuel_core::service::FuelService;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run_cli().await
}
