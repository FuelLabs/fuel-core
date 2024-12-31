use std::{
    ops::Range,
    path::{
        Path,
        PathBuf,
    },
};

use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;
use reqwest::{
    header::{
        HeaderMap,
        CONTENT_TYPE,
    },
    Response,
    Url,
};

use clap::{
    ArgGroup,
    Args,
    Parser,
    Subcommand,
};

use serde::{
    Deserialize,
    Serialize,
};
use serde_json;

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
    } = Arg::parse();
    // Safety: The block range is always a vector of length 2
    let start_block_included = block_range[0];
    let end_block_excluded = block_range[1];
    let block_range = start_block_included..end_block_excluded;

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
        fetch_block_committer_data(&block_committer_data_fetcher, block_range).await?;

    println!("{:?}", block_costs);
    Ok(())
}

async fn fetch_block_committer_data(
    data_fetcher: &BlockCommitterDataFetcher,
    blocks: Range<u64>,
) -> Result<Vec<BlockCommitterCosts>, anyhow::Error> {
    let mut block_costs = vec![];
    let mut current_block_height = blocks.start;
    while current_block_height < blocks.end {
        let Ok(mut costs) = data_fetcher.fetch_blob_data(current_block_height).await
        else {
            Err(anyhow::anyhow!(
                "Could not fetch data for block {}",
                current_block_height
            ))?
        };

        if costs.is_empty() {
            // Might be that the block committer doesn't have data for the block, in which case we return prematurely.
            // If this happen, we should increase the value of results returned by the block committer in the query.
            break;
        }

        // Block committer will return the data for the block in the next batch, hence we don't increment the height of the last
        // block.
        current_block_height = costs.last().unwrap().end_height;
        block_costs.append(&mut costs);
    }

    Ok(block_costs)
}

#[derive(Debug, Serialize, Deserialize)]
struct BlockCommitterCosts {
    /// The cost of the block, supposedly in Wei but need to check
    cost: u128,
    size: u64,
    da_block_height: u64,
    start_height: u64,
    end_height: u64,
}

struct BlockCommitterDataFetcher {
    client: reqwest::Client,
    endpoint: Url,
    num_responses: usize,
}

impl BlockCommitterDataFetcher {
    fn new(endpoint: Url, num_responses: usize) -> Result<Self, anyhow::Error> {
        let mut content_type_json_header = HeaderMap::new();
        content_type_json_header.insert(
            CONTENT_TYPE,
            "application/json"
                .parse()
                .expect("Content-Type header value is valid"),
        );
        let client = reqwest::ClientBuilder::new()
            .default_headers(content_type_json_header)
            .build()?;
        Ok(Self {
            client,
            endpoint,
            num_responses,
        })
    }

    // Todo: Better error type
    async fn fetch_blob_data(
        &self,
        from_height: u64,
    ) -> Result<Vec<BlockCommitterCosts>, anyhow::Error> {
        let query = self.endpoint.join("v1/costs")?.join(&format!(
            "?from_height={}&limit={}",
            from_height, self.num_responses
        ))?;

        println!("Query: {}", query.as_str());

        let response = self.client.get(query).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch data from block committer: {}",
                response.status(),
            )
            .into());
        }

        let block_committer_costs = response.json::<Vec<BlockCommitterCosts>>().await?;
        Ok(block_committer_costs)
    }
}
