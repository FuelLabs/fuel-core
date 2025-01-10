use crate::{
    data::get_data,
    display::display_results,
    service::get_service_controller,
    simulation::simulation,
};
use clap::{
    Parser,
    Subcommand,
};
use itertools::Itertools;
use plotters::prelude::*;
use std::{
    env,
    path::PathBuf,
};
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    Layer,
};

pub mod data;
pub mod data_sources;
pub mod display;
pub mod service;
pub mod simulation;

#[derive(Subcommand, Debug)]
pub enum DataSource {
    /// Load data from a CSV file.
    File {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Generate arbitrary data (not supported yet).
    Generated,
}

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    data_source: DataSource,
}

fn configure_tracing() {
    let filter = match env::var_os("RUST_LOG") {
        Some(_) => {
            EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided")
        }
        None => EnvFilter::new("info"),
    };

    let fmt = tracing_subscriber::fmt::Layer::default()
        .with_level(true)
        .boxed();

    tracing_subscriber::registry().with(fmt).with(filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();

    let args = Args::parse();
    let data = get_data(&args.data_source)?;
    let starting_height = data.starting_height();
    let mut service_controller = get_service_controller(starting_height).await?;
    let results = simulation(&data, &mut service_controller).await?;
    display_results(results)?;
    Ok(())
}
