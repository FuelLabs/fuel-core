#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

use crate::{
    cli::Args,
    data::get_data,
    display::display_results,
    simulation::run_a_simulation,
    tracing::configure_tracing,
};
use clap::Parser;

pub mod cli;
pub mod data;
pub mod data_sources;

#[allow(clippy::arithmetic_side_effects)]
#[allow(clippy::cast_possible_truncation)]
pub mod display;
pub mod service;
pub mod simulation;
pub mod tracing;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    let args = Args::parse();
    let data = get_data(&args.file_path)?;
    let results = run_a_simulation(args.simulation, data).await?;
    display_results(results)?;
    Ok(())
}
