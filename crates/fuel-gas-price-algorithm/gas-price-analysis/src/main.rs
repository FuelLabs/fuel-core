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
        run_simulation,
        SimulationResults,
    },
};

mod optimisation;
mod simulation;

mod charts;

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
    Generated { size: usize }
}

fn main() {
    let args = Arg::parse();

    let (results, (p_comp, d_comp)) = match args.mode {
        Mode::WithValues { p, d, source } => {
            let Source::Generated { size } = source;
            println!("Running simulation with P: {}, D: {}, and {} blocks", pretty(p), pretty(d), pretty(size));
            let result = run_simulation(p, d, size);
            (result, (p, d))
        },
        Mode::Optimization { iterations, source} => {
            let Source::Generated { size } = source;
            println!("Running optimization with {iterations} iterations and {size} blocks");
            let (results, (p, d)) = naive_optimisation(iterations as usize, size);
            println!("Optimization results: P: {}, D: {}", pretty(p), pretty(d));
            (results, (p, d))
        }
    };

    if let Some(file_path) = &args.file_path {
        draw_chart(results, p_comp, d_comp, file_path);

    }
}

fn draw_chart(results: SimulationResults, p_comp: i64, d_comp: i64, file_path: &str) {
    let SimulationResults {
        gas_prices,
        exec_gas_prices,
        da_gas_prices,
        fullness,
        bytes_and_costs,
        actual_profit,
        projected_profit,
        pessimistic_costs,
    } = results;

    let max_actual_profit = pretty(*actual_profit.iter().max().unwrap() as u64);
    println!("max_actual: {max_actual_profit}");

    let plot_width = 640 * 2 * 2;
    let plot_height = 480 * 3;

    let root =
        BitMapBackend::new(file_path, (plot_width, plot_height)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (window_one, lower) = root.split_vertically(plot_height / 4);
    let (window_two, new_lower) = lower.split_vertically(plot_height / 4);
    let (window_three, window_four) = new_lower.split_vertically(plot_height / 4);

    draw_fullness(&window_one, &fullness, "Fullness");

    draw_bytes_and_cost_per_block(&window_two, &bytes_and_costs, "Bytes Per Block");

    draw_profit(
        &window_three,
        &actual_profit,
        &projected_profit,
        &pessimistic_costs,
        &format!("Profit p_comp: {p_comp:?}, d_comp: {d_comp:?}"),
    );
    draw_gas_prices(
        &window_four,
        &gas_prices,
        &exec_gas_prices,
        &da_gas_prices,
        "Gas Prices",
    );

    root.present().unwrap();
}

