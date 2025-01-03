use std::path::PathBuf;

use layer1::BlockCommitterDataFetcher;
use layer2::Layer2BlockData;
use reqwest::{
    header::{
        HeaderMap,
        CONTENT_TYPE,
    },
    Url,
};

use clap::{
    Args,
    Parser,
};

pub mod client_ext;
mod layer1;
mod layer2;
mod summary;

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
    block_range: Vec<u64>,

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
    // Safety: The block range is always a vector of length 2
    let start_block_included = block_range[0];
    let end_block_excluded = block_range[1];
    let block_range: std::ops::Range<u64> =
        start_block_included.saturating_sub(3600)..end_block_excluded;

    if end_block_excluded < start_block_included {
        return Err(anyhow::anyhow!(
            "Invalid block range - start block must be lower than end block: {}..{}",
            start_block_included,
            end_block_excluded
        ));
    }
    let mut content_type_json_header = HeaderMap::new();
    content_type_json_header.insert(
        CONTENT_TYPE,
        "application/json"
            .parse()
            .expect("Content-Type header value is valid"),
    );

    let block_committer_data_fetcher =
        BlockCommitterDataFetcher::new(block_committer_endpoint, 10)?;

    let block_costs =
        layer1::fetch_block_committer_data(&block_committer_data_fetcher, block_range)
            .await?;

    println!("{:?}", block_costs);
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
            println!(
                "Retrieving L2 data from sentry node: {}",
                sentry_node_endpoint
            );
            let sentry_node_client = layer2::BlockFetcher::new(sentry_node_endpoint)?;
            let blocks_range =
                start_block_included.try_into()?..end_block_excluded.try_into()?;

            let blocks_with_gas_consumed =
                layer2::get_gas_consumed(&sentry_node_client, blocks_range, 5000).await?;

            for (
                _block_height,
                Layer2BlockData {
                    block_height,
                    block_size,
                    gas_consumed,
                    ..
                },
            ) in &blocks_with_gas_consumed
            {
                println!(
                    "Block Height: {}, Block Size: {}, Gas Consumed: {}",
                    block_height, block_size, gas_consumed
                );
            }
            summary::summarise_available_data(
                &output_file,
                &block_costs,
                &blocks_with_gas_consumed,
            )
            .inspect_err(|e| {
                println!("Failed to write to CSV file: {:?}, {:?}", output_file, e)
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
