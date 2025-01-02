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

fn fullness_and_bytes_per_block(size: usize, capacity: u64) -> Vec<(u64, u32)> {
    let mut rng = StdRng::seed_from_u64(888);

    let fullness_noise: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(-0.5..0.5))
        // .map(|val| val * capacity as f64)
        .collect();

    const ROUGH_GAS_TO_BYTE_RATIO: f64 = 0.01;
    let bytes_scale: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(0.5..1.0))
        .map(|x| x * ROUGH_GAS_TO_BYTE_RATIO)
        .collect();

    (0usize..size)
        .map(|val| val as f64)
        .map(noisy_fullness)
        .map(|signal| (0.01 * signal + 0.01) * capacity as f64) // Scale and shift so it's between 0 and capacity
        .zip(fullness_noise)
        .map(|(fullness, noise)| fullness + noise)
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| f64::max(x, 5.0))
        .zip(bytes_scale)
        .map(|(fullness, bytes_scale)| {
            let bytes = fullness * bytes_scale;
            (fullness, bytes)
        })
        .map(|(fullness, bytes)| (fullness as u64, std::cmp::max(bytes as u32, 1)))
        .collect()
}

// Naive Fourier series
fn gen_noisy_signal(input: f64, components: &[f64]) -> f64 {
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
        / components.len() as f64
}

fn noisy_fullness<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[-30.0, 40.0, 700.0, -340.0, 400.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}

struct L1FullnessData {
    da_cost_per_byte: Vec<u64>,
    fullness_and_bytes: Vec<(u64, u32)>,
}

fn fulness_and_bytes_from_source(
    source: Source,
    capacity: u64,
    update_period: usize,
) -> L1FullnessData {
    let da_cost_per_byte = get_da_cost_per_byte_from_source(source, update_period);
    let size = da_cost_per_byte.len();
    L1FullnessData {
        da_cost_per_byte,
        fullness_and_bytes: fullness_and_bytes_per_block(size, capacity),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arg::parse();

    const UPDATE_PERIOD: usize = 12;
    const CAPACITY: u64 = 30_000_000;

    let da_finalization_period = args.da_finalization_period;

    let (results, (p_comp, d_comp)) = match args.mode {
        Mode::WithValues { p, d, source } => {
            let L1FullnessData {
                da_cost_per_byte,
                fullness_and_bytes,
            } = fulness_and_bytes_from_source(source, CAPACITY, UPDATE_PERIOD);

            println!(
                "Running simulation with P: {}, D: {}, {} blocks and {} da_finalization_period",
                prettify_number(p),
                prettify_number(d),
                prettify_number(da_cost_per_byte.len()),
                prettify_number(da_finalization_period)
            );
            let simulator = Simulator::new(da_cost_per_byte);
            let result = simulator.run_simulation(
                p,
                d,
                UPDATE_PERIOD,
                &fullness_and_bytes,
                da_finalization_period,
            );
            (result, (p, d))
        }
        Mode::Optimization { iterations, source } => {
            let L1FullnessData {
                da_cost_per_byte,
                fullness_and_bytes,
            } = fulness_and_bytes_from_source(source, CAPACITY, UPDATE_PERIOD);
            println!(
                "Running optimization with {iterations} iterations and {} blocks",
                da_cost_per_byte.len()
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
