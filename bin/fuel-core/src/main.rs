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

#[cfg(test)]
mod tests {
    // No other way to explain that the following deps are used in tests only when snapshots are activated
    // (that is when rocksdb or rocksdb-production is available).
    use fuel_core_storage as _;
    use itertools as _;
    use pretty_assertions as _;
    use rand as _;
    use serde as _;
    use tempfile as _;
}
