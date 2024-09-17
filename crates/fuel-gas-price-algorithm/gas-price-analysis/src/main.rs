use crate::{
    charts::draw_chart,
    simulation::da_cost_per_byte::get_da_cost_per_byte_from_source,
};
use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;

use crate::{
    optimisation::naive_optimisation,
    simulation::{
        SimulationResults,
        Simulator,
    },
};

mod optimisation;
mod simulation;

mod charts;

use clap::{
    Parser,
    Subcommand,
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Arg {
    #[command(subcommand)]
    mode: Mode,
    /// File path to save the chart to. Will not generate chart if left blank
    #[arg(short, long)]
    file_path: Option<String>,
}

#[derive(Subcommand)]
enum Mode {
    /// Run the simulation with the given P and D values
    WithValues {
        /// Source of blocks
        #[command(subcommand)]
        source: Source,
        /// P value
        p: i64,
        /// D value
        d: i64,
    },
    /// Run an optimization to find the best P and D values
    Optimization {
        /// Source of blocks
        #[command(subcommand)]
        source: Source,
        /// Number of iterations to run the optimization for
        iterations: u64,
    },
}

#[derive(Subcommand)]
enum Source {
    Generated {
        size: usize,
    },
    Predefined {
        file_path: String,
        /// The number of L2 blocks to include from source
        #[arg(short, long)]
        sample_size: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arg::parse();

    const UPDATE_PERIOD: usize = 12;

    let (results, (p_comp, d_comp)) = match args.mode {
        Mode::WithValues { p, d, source } => {
            let da_cost_per_byte =
                get_da_cost_per_byte_from_source(source, UPDATE_PERIOD);
            let size = da_cost_per_byte.len();
            println!(
                "Running simulation with P: {}, D: {}, and {} blocks",
                prettify_number(p),
                prettify_number(d),
                prettify_number(size)
            );
            let simulator = Simulator::new(da_cost_per_byte);
            let result = simulator.run_simulation(p, d, UPDATE_PERIOD);
            (result, (p, d))
        }
        Mode::Optimization { iterations, source } => {
            let da_cost_per_byte =
                get_da_cost_per_byte_from_source(source, UPDATE_PERIOD);
            let size = da_cost_per_byte.len();
            println!(
                "Running optimization with {iterations} iterations and {size} blocks"
            );
            let simulator = Simulator::new(da_cost_per_byte);
            let (results, (p, d)) =
                naive_optimisation(&simulator, iterations as usize, UPDATE_PERIOD).await;
            println!(
                "Optimization results: P: {}, D: {}",
                prettify_number(p),
                prettify_number(d)
            );
            (results, (p, d))
        }
    };

    print_info(&results);

    if let Some(file_path) = &args.file_path {
        draw_chart(results, p_comp, d_comp, file_path)?;
    }

    Ok(())
}

fn print_info(results: &SimulationResults) {
    let SimulationResults {
        da_gas_prices,
        actual_profit,
        projected_profit,
        ..
    } = results;

    // Max actual profit
    let (index, max_actual_profit) = actual_profit
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *max_actual_profit as f64 / (10_f64).powf(18.);
    println!("max actual profit: {} ETH at {}", eth, index);

    // Max projected profit
    let (index, max_projected_profit) = projected_profit
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *max_projected_profit as f64 / (10_f64).powf(18.);
    println!("max projected profit: {} ETH at {}", eth, index);

    // Min actual profit
    let (index, min_actual_profit) = actual_profit
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *min_actual_profit as f64 / (10_f64).powf(18.);
    println!("min actual profit: {} ETH at {}", eth, index);

    // Min projected profit
    let (index, min_projected_profit) = projected_profit
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *min_projected_profit as f64 / (10_f64).powf(18.);
    println!("min projected profit: {} ETH at {}", eth, index);

    // Max DA Gas Price
    let (index, max_da_gas_price) = da_gas_prices
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *max_da_gas_price as f64 / (10_f64).powf(18.);
    println!(
        "max DA gas price: {} Wei ({} ETH) at {}",
        max_da_gas_price, eth, index
    );

    // Min Da Gas Price
    let (index, min_da_gas_price) = da_gas_prices
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *min_da_gas_price as f64 / (10_f64).powf(18.);
    println!(
        "min DA gas price: {} Wei ({} ETH) at {}",
        min_da_gas_price, eth, index
    );
}

pub fn prettify_number<T: std::fmt::Display>(input: T) -> String {
    input
        .to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(",") // separator
}
