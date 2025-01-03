use std::path::Path;

use crate::{
    charts::draw_chart,
    simulation::da_cost_per_byte::get_da_cost_per_byte_from_source,
};
use block_data::{
    arb_l2_fullness_and_bytes_per_block,
    Predefined2Record,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;
use utils::fields_of_struct_in_order;

use crate::{
    optimisation::naive_optimisation,
    simulation::{
        SimulationResults,
        Simulator,
    },
};

mod block_data;
mod charts;
mod optimisation;
mod simulation;
mod utils;

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
    #[arg(short, long)]
    /// DA finalization period in L2 blocks
    da_finalization_period: usize,
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
    /// Generate arbitrary blocks
    Generated {
        /// Number of blocks to generate
        size: usize,
    },
    /// Use predefined L2 block data from file (legacy format)
    Predefined {
        /// Path to the file containing predefined blocks (.csv file with the following
        /// columns: block_number, excess_blob_gas, blob_gas_used, blob_fee_wei,
        /// blob_fee_wei_for_1_blob, blob_fee_wei_for_2_blobs, blob_fee_wei_for_3_blobs)
        file_path: String,
        /// The number of blocks to include from source, specified in L1 blocks.
        /// This algorithm assumes that there are 12 L2 blocks per L1 block, so if you
        /// pass 1200 here, it will pull 100 L1 blocks from the source file.
        #[arg(short, long)]
        sample_size: Option<usize>,
    },
    /// Use predefined L1 and L2 block data from a file
    Predefined2 {
        /// Path to the file containing predefined blocks (.csv file with the following
        /// columns: L1_block_number, L1_blob_fee_wei, L2_block_number, L2_fulness, L2_size)
        file_path: String,
        /// The number of L2 blocks per single L1 blob
        #[arg(short, long)]
        l2_blocks_per_blob: usize,
    },
}

fn get_l2_costs_from_csv_file<P: AsRef<Path>>(file_path: P) -> Vec<(u64, u32, u64)> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(file_path)
        .unwrap();
    let headers =
        csv::StringRecord::from(fields_of_struct_in_order::<Predefined2Record>());

    rdr.records()
        .map(|record| {
            let record: Predefined2Record =
                record.unwrap().deserialize(Some(&headers)).unwrap();
            (
                record.l2_fullness(),
                record.l2_size() as u32,
                record.l2_block_number(),
            )
        })
        .collect()
}

// TODO[RC]: Rename, to be more descriptive (not confusing with L1L2BlockData and BlockData)
// TODO[RC]: Into `BlockData` instead of manual mapping in `run_simulation()`
#[derive(Debug, Clone)]
struct L2BlockData {
    height: u64,
    fullness: u64,
    size: u32,
}

// TODO[RC]: Rename, to be more descriptive (not confusing with L2BlockData and BlockData)
struct L1L2BlockData {
    da_cost_per_byte: Vec<u64>,
    fullness_and_bytes: Vec<L2BlockData>,
}

fn l1_l2_block_data_from_source(
    source: &Source,
    capacity: u64,
    update_period: usize,
) -> L1L2BlockData {
    let da_cost_per_byte = get_da_cost_per_byte_from_source(&source, update_period);
    let size = da_cost_per_byte.len();

    let fullness_and_bytes = match source {
        Source::Generated { .. } | Source::Predefined { .. } => {
            arb_l2_fullness_and_bytes_per_block(size, capacity)
        }
        Source::Predefined2 { file_path, .. } => get_l2_costs_from_csv_file(file_path),
    }
    .iter()
    .map(|(fullness, size, height)| L2BlockData {
        fullness: *fullness,
        size: *size,
        height: *height,
    })
    .collect();

    L1L2BlockData {
        da_cost_per_byte,
        fullness_and_bytes,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arg::parse();

    tracing_subscriber::fmt::init();

    const UPDATE_PERIOD: usize = 12;
    const CAPACITY: u64 = 30_000_000;

    let da_finalization_period = args.da_finalization_period;

    let (results, (p_comp, d_comp)) = match args.mode {
        Mode::WithValues { p, d, source } => {
            let L1L2BlockData {
                da_cost_per_byte,
                fullness_and_bytes,
            } = l1_l2_block_data_from_source(&source, CAPACITY, UPDATE_PERIOD);

            dbg!(&fullness_and_bytes);

            println!(
                "Running simulation with P: {}, D: {}, {} L2 blocks and {} da_finalization_period",
                prettify_number(p),
                prettify_number(d),
                prettify_number(fullness_and_bytes.len()),
                prettify_number(da_finalization_period)
            );
            let simulator = Simulator::new(da_cost_per_byte);

            let update_period = match source {
                Source::Generated { .. } | Source::Predefined { .. } => UPDATE_PERIOD,
                Source::Predefined2 {
                    l2_blocks_per_blob, ..
                } => l2_blocks_per_blob,
            };

            let result = simulator.run_simulation(
                p,
                d,
                update_period,
                &fullness_and_bytes,
                da_finalization_period,
            );
            (result, (p, d))
        }
        Mode::Optimization { iterations, source } => {
            let L1L2BlockData {
                da_cost_per_byte,
                fullness_and_bytes,
            } = l1_l2_block_data_from_source(&source, CAPACITY, UPDATE_PERIOD);
            println!(
                "Running optimization with {iterations} iterations and {} L2 blocks",
                fullness_and_bytes.len()
            );
            let simulator = Simulator::new(da_cost_per_byte);
            let (results, (p, d)) = naive_optimisation(
                &simulator,
                iterations as usize,
                UPDATE_PERIOD,
                &fullness_and_bytes,
                da_finalization_period,
            )
            .await;
            println!(
                "Optimization results: P: {}, D: {}, da_finalization_period: {}",
                prettify_number(p),
                prettify_number(d),
                prettify_number(da_finalization_period)
            );
            (results, (p, d))
        }
    };

    print_info(&results);

    if let Some(file_path) = &args.file_path {
        draw_chart(results, p_comp, d_comp, da_finalization_period, file_path)?;
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
    let eth = *max_da_gas_price as f64 / (10_f64).powf(9.);
    println!(
        "max DA gas price: {} Wei ({} Gwei) at {}",
        max_da_gas_price, eth, index
    );

    // Min Da Gas Price
    let (index, min_da_gas_price) = da_gas_prices
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .unwrap();
    let eth = *min_da_gas_price as f64 / (10_f64).powf(9.);
    println!(
        "min DA gas price: {} Wei ({} Gwei) at {}",
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
