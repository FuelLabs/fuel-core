use crate::{
    cli::{
        Args,
        SimulationArgs,
    },
    data::{
        get_data,
        Data,
    },
    display::display_results,
    service::{
        get_service_controller,
        ConfigValues,
        MetadataValues,
    },
    simulation::{
        single_simulation,
        SimulationResults,
    },
    tracing::configure_tracing,
};
use clap::Parser;

pub mod cli;
pub mod data;
pub mod data_sources;
pub mod display;
pub mod service;
pub mod simulation;
pub mod tracing;

pub async fn run_single_simulation(data: Data) -> anyhow::Result<SimulationResults> {
    let starting_height = data.starting_height();
    let config_values = ConfigValues {
        min_da_gas_price: 1000,
        max_da_gas_price: u64::MAX,
        da_p_component: 50_000_000__000_000_000,
        da_d_component: 100_000__000_000_000,
    };
    let metadata_values = MetadataValues::new(starting_height, 18_963);
    let mut service_controller =
        get_service_controller(config_values, metadata_values).await?;
    let results = single_simulation(&data, &mut service_controller).await?;
    Ok(results)
}

pub async fn run_a_simulation(
    args: SimulationArgs,
    data: Data,
) -> anyhow::Result<SimulationResults> {
    match args {
        SimulationArgs::Single {
            p_component,
            d_component,
            start_gas_price,
        } => {
            let starting_height = data.starting_height();
            let config_values = ConfigValues {
                min_da_gas_price: 1000,
                max_da_gas_price: u64::MAX,
                da_p_component: p_component,
                da_d_component: d_component,
            };
            let metadata_values = MetadataValues::new(starting_height, start_gas_price);
            let mut service_controller =
                get_service_controller(config_values, metadata_values).await?;
            let results = single_simulation(&data, &mut service_controller).await?;
            Ok(results)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();

    let args = Args::parse();
    let data = get_data(&args.file_path)?;
    let results = run_single_simulation(data).await?;
    display_results(results)?;
    Ok(())
}
