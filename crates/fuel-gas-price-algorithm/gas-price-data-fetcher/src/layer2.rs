use std::{
    collections::{
        HashMap,
        HashSet,
    },
    ops::Range,
};

use super::client_ext::{
    ClientExt,
    SealedBlockWithMetadata,
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};
use fuel_core_types::{
    fuel_tx::{
        Chargeable,
        ConsensusParameters,
    },
    fuel_types::BlockHeight,
};
use itertools::Itertools;

pub struct Layer2BlockData {
    pub block_height: BlockHeight,
    pub block_size: usize,
    pub gas_consumed: u64,
    pub capacity: u64,
    pub bytes_capacity: u64,
}

#[derive(Clone)]
pub struct BlockFetcher {
    client: FuelClient,
}

impl BlockFetcher {
    pub fn new(url: impl AsRef<str>) -> anyhow::Result<Self> {
        let client = FuelClient::new(url)?;
        Ok(Self { client })
    }
}

impl BlockFetcher {
    pub async fn last_height(&self) -> anyhow::Result<BlockHeight> {
        let chain_info = self.client.chain_info().await?;
        let height = chain_info.latest_block.header.height.into();

        Ok(height)
    }

    pub async fn blocks_for(
        &self,
        range: Range<u32>,
    ) -> anyhow::Result<Vec<SealedBlockWithMetadata>> {
        if range.is_empty() {
            return Ok(vec![]);
        }

        let start = range.start.saturating_sub(1);
        let size = i32::try_from(range.len()).expect("Should be a valid i32");

        let request = PaginationRequest {
            cursor: Some(start.to_string()),
            results: size,
            direction: PageDirection::Forward,
        };
        let response = self.client.full_blocks(request).await?;
        let blocks = response
            .results
            .into_iter()
            .map(TryInto::try_into)
            .try_collect()?;
        Ok(blocks)
    }
}

fn height(block: &SealedBlockWithMetadata) -> BlockHeight {
    *block.block.entity.header().height()
}

fn total_gas_consumed(
    block: &SealedBlockWithMetadata,
    consensus_parameters: &ConsensusParameters,
) -> Result<u64, anyhow::Error> {
    let min_gas: u64 = block
        .block
        .entity
        .transactions()
        .iter()
        .filter_map(|tx| match tx {
            fuel_core_types::fuel_tx::Transaction::Script(chargeable_transaction) => {
                Some(chargeable_transaction.min_gas(
                    consensus_parameters.gas_costs(),
                    consensus_parameters.fee_params(),
                ))
            }
            fuel_core_types::fuel_tx::Transaction::Create(chargeable_transaction) => {
                Some(chargeable_transaction.min_gas(
                    consensus_parameters.gas_costs(),
                    consensus_parameters.fee_params(),
                ))
            }
            fuel_core_types::fuel_tx::Transaction::Mint(_mint) => None,
            fuel_core_types::fuel_tx::Transaction::Upgrade(chargeable_transaction) => {
                Some(chargeable_transaction.min_gas(
                    consensus_parameters.gas_costs(),
                    consensus_parameters.fee_params(),
                ))
            }
            fuel_core_types::fuel_tx::Transaction::Upload(chargeable_transaction) => {
                Some(chargeable_transaction.min_gas(
                    consensus_parameters.gas_costs(),
                    consensus_parameters.fee_params(),
                ))
            }
            fuel_core_types::fuel_tx::Transaction::Blob(chargeable_transaction) => {
                Some(chargeable_transaction.min_gas(
                    consensus_parameters.gas_costs(),
                    consensus_parameters.fee_params(),
                ))
            }
        })
        .sum();
    let gas_consumed = block
        .receipts
        .iter()
        .flatten()
        .map(|r| r.iter().filter_map(|r| r.gas_used()).sum::<u64>())
        .sum();
    min_gas
        .checked_add(gas_consumed)
        .ok_or(anyhow::anyhow!("Gas overflow"))
}

pub async fn get_gas_consumed(
    block_fetcher: &BlockFetcher,
    range: Range<u32>,
    num_results: usize,
) -> anyhow::Result<HashMap<BlockHeight, Layer2BlockData>> {
    let mut ranges: Vec<Range<u32>> =
        Vec::with_capacity(range.len().saturating_div(num_results));

    for start in range.clone().step_by(num_results) {
        let end = start.saturating_add(num_results as u32).min(range.end);
        ranges.push(start..end);
    }

    let mut blocks = Vec::with_capacity(range.len());
    for range in ranges {
        println!("Fetching blocks for range {:?}", range);
        let blocks_for_range = block_fetcher.blocks_for(range).await?;
        blocks.extend(blocks_for_range);
    }

    let consensus_parameters_versions = blocks
        .iter()
        .map(|b| b.block.entity.header().consensus_parameters_version)
        .collect::<HashSet<u32>>();

    println!(
        "Consensus parameter versions: {:?}",
        consensus_parameters_versions
    );

    let mut consensus_parameters: HashMap<u32, ConsensusParameters> = HashMap::new();
    for consensus_parameters_version in consensus_parameters_versions {
        let cp = block_fetcher
            .client
            .consensus_parameters(consensus_parameters_version.try_into()?)
            .await?;

        if let Some(cp) = cp {
            println!(
                "Found consensus parameters for version {}: {:?}",
                consensus_parameters_version, cp
            );
            consensus_parameters.insert(consensus_parameters_version, cp);
        }
    }

    let mut block_data = HashMap::with_capacity(range.len());

    for b in blocks {
        let block_height = height(&b);
        let consensus_parameters = consensus_parameters
            .get(&b.block.entity.header().consensus_parameters_version)
            .ok_or(anyhow::anyhow!(
                "Consensus parameters not found for block {}",
                block_height
            ))?;
        // let compressed_block = b.block.entity.compress(&consensus_parameters.chain_id());
        let block_size = postcard::to_allocvec(&b.block)?.len();

        let gas_consumed = total_gas_consumed(&b, consensus_parameters)?;
        let capacity = consensus_parameters.block_gas_limit();
        let bytes_capacity = consensus_parameters.block_transaction_size_limit();

        block_data.insert(
            block_height,
            Layer2BlockData {
                block_height,
                block_size,
                gas_consumed,
                capacity,
                bytes_capacity,
            },
        );
    }

    Ok(block_data)
}
