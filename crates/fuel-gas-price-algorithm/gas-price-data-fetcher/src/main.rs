use std::{
    env,
    path::PathBuf,
};

use fuel_core_types::fuel_types::BlockHeight;
use layer1::BlockCommitterDataFetcher;
use reqwest::Url;
use tracing_subscriber::EnvFilter;
use types::Layer2BlockData;

use clap::{
    Args,
    Parser,
};

use tracing_subscriber::prelude::*;

pub mod client_ext;
mod layer1;
mod layer2;
mod summary;
mod types;

// const SENTRY_NODE_GRAPHQL_RESULTS_PER_QUERY: usize = 5_000;
const SENTRY_NODE_GRAPHQL_RESULTS_PER_QUERY: usize = 40;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Arg {
    #[clap(flatten)]
    l2_block_data_source: L2BlockDataSource,
    #[arg(short, long)]
    /// Endpoint of the block committer to fetch L1 blobs data
    block_committer_endpoint: Url,
    #[arg(
        short = 'r',
        long,
        num_args = 2..=2,
        required = true
    )]
    /// Range of blocks to fetch the data for. Lower bound included, Upper bound excluded.
    block_range: Vec<u32>,

    #[arg(required = true)]
    /// The output CSV file where Block and Blob data will be written to
    output_file: PathBuf,
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
struct L2BlockDataSource {
    #[arg(short, long)]
    /// Path of the database stored by a fuel-node to retrieve L2 block data. Alternatively, the endpoint of a sentry node can be provided using --sentry-node-endpoint.
    db_path: Option<PathBuf>,
    #[arg(short, long)]
    /// Endpoint of the sentry node to fetch L2 block data. Alternatively, the path of the database stored by a fuel-node can be provided using --db-path.
    sentry_node_endpoint: Option<Url>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Arg {
        block_committer_endpoint,
        l2_block_data_source,
        block_range,
        output_file,
    } = Arg::parse();

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

    // Safety: The block range is always a vector of length 2
    let start_block_included = BlockHeight::from(block_range[0]);
    // When requested a set of results, the block committer will fetch the data for the next blob which
    // might not include the current height. Each blob contains 3_600 blocks, hence we subtract
    // this amount from the block height we use for the first request.
    let end_block_excluded = BlockHeight::from(block_range[1]);
    let block_range = start_block_included..end_block_excluded;

    if end_block_excluded < start_block_included {
        return Err(anyhow::anyhow!(
            "Invalid block range - start block must be lower than end block: {}..{}",
            start_block_included,
            end_block_excluded
        ));
    }

    let block_committer_data_fetcher =
        BlockCommitterDataFetcher::new(block_committer_endpoint, 10)?;

    let block_costs = block_committer_data_fetcher
        .fetch_l1_block_costs(block_range)
        .await?;

    tracing::debug!("{:?}", block_costs);
    match l2_block_data_source {
        L2BlockDataSource {
            db_path: Some(_db_path),
            sentry_node_endpoint: None,
        } => {
            todo!();
        }
        L2BlockDataSource {
            db_path: None,
            sentry_node_endpoint: Some(sentry_node_endpoint),
        } => {
            tracing::info!(
                "Retrieving L2 data from sentry node: {}",
                sentry_node_endpoint
            );
            let sentry_node_client = layer2::BlockFetcher::new(sentry_node_endpoint)?;
            let blocks_range = start_block_included..end_block_excluded;

            tracing::info!(
                "Retrieving L2 data for blocks: {}..{}",
                start_block_included,
                end_block_excluded
            );
            let res = sentry_node_client
                .get_l2_block_data(blocks_range, SENTRY_NODE_GRAPHQL_RESULTS_PER_QUERY)
                .await;
            tracing::info!("results: {:?}", res);

            let blocks_with_gas_consumed = res?;
            for (
                _block_height,
                Layer2BlockData {
                    block_height,
                    block_size,
                    gas_consumed,
                    capacity,
                    bytes_capacity,
                    transactions_count,
                },
            ) in &blocks_with_gas_consumed
            {
                tracing::debug!(
                    "Block Height: {}, Block Size: {}, Gas Consumed: {}, Capacity: {}, Bytes Capacity: {}, Transactions count: {}",
                    block_height, **block_size, **gas_consumed, **capacity, **bytes_capacity, transactions_count
                );
            }
            summary::summarise_available_data(
                &output_file,
                &block_costs,
                &blocks_with_gas_consumed,
            )
            .inspect_err(|e| {
                tracing::error!("Failed to write to CSV file: {:?}, {:?}", output_file, e)
            })?;
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Either db-path or sentry-node-endpoint must be provided"
            ));
        }
    };

    Ok(())
}
