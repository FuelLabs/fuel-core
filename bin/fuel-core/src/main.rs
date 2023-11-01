#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

// Use Jemalloc for main binary
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use fuel_core::service::FuelService;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run_cli().await
}
