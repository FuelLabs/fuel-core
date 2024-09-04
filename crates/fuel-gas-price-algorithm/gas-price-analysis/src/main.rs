use crate::charts::draw_chart;
use crate::simulation::get_da_cost_per_byte_from_source;
use crate::simulation::{arbitrary_cost_per_byte,};
use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;

use crate::{
    charts::{
        draw_bytes_and_cost_per_block,
        draw_fullness,
        draw_gas_prices,
        draw_profit,
    },
    optimisation::naive_optimisation,
    simulation::{
        Simulator,
        SimulationResults,
    },
};

mod optimisation;
mod simulation;

mod charts;



use clap::{Parser, Subcommand};

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
    }
}

#[derive(Subcommand)]
enum Source {
    Generated { size: usize },
    Predefined { file_path: String }
}

fn main() {
    let args = Arg::parse();

    const UPDATE_PERIOD: usize = 12;

    let (results, (p_comp, d_comp)) = match args.mode {
        Mode::WithValues { p, d, source } => {
            let da_cost_per_byte = get_da_cost_per_byte_from_source(source, UPDATE_PERIOD);
            let size = da_cost_per_byte.len();
            println!("Running simulation with P: {}, D: {}, and {} blocks", pretty(p), pretty(d), pretty(size));
            let simulator = Simulator::new(da_cost_per_byte);
            let result = simulator.run_simulation(p, d, UPDATE_PERIOD);
            (result, (p, d))
        },
        Mode::Optimization { iterations, source} => {
            let da_cost_per_byte = get_da_cost_per_byte_from_source(source, UPDATE_PERIOD);
            let size = da_cost_per_byte.len();
            println!("Running optimization with {iterations} iterations and {size} blocks");
            let simulator = Simulator::new(da_cost_per_byte);
            let (results, (p, d)) = naive_optimisation(simulator, iterations as usize, UPDATE_PERIOD);
            println!("Optimization results: P: {}, D: {}", pretty(p), pretty(d));
            (results, (p, d))
        }
    };

    if let Some(file_path) = &args.file_path {
        draw_chart(results, p_comp, d_comp, file_path);

    }
}

pub fn pretty<T: ToString>(input: T) -> String {
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
