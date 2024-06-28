use anyhow::anyhow;
use fuel_core_gas_price_service::fuel_gas_price_updater::{
    BlockInfo,
    Error as GasPriceError,
    L2BlockSource,
    Result as GasPriceResult,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    tables::{
        ConsensusParametersVersions,
        FuelBlocks,
        Transactions,
    },
    transactional::AtomicView,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::{
        field::{
            MintAmount,
            MintGasPrice,
        },
        ConsensusParameters,
        Transaction,
    },
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use tokio_stream::StreamExt;

#[cfg(test)]
mod tests;

pub struct FuelL2BlockSource<Database> {
    database: Database,
    committed_block_stream: BoxStream<SharedImportResult>,
}

fn get_block_info(
    block: &Block<Transaction>,
    consensus_params: &ConsensusParameters,
) -> GasPriceResult<BlockInfo> {
    let gas_price_factor = consensus_params.fee_params().gas_price_factor();
    let used_gas = block_used_gas(block, gas_price_factor)?;
    let block_gas_limit = consensus_params.block_gas_limit();
    let info = BlockInfo {
        height: (*block.header().height()).into(),
        fullness: (used_gas, block_gas_limit),
        block_bytes: 0,
        gas_price: 0,
    };
    Ok(info)
}

fn block_used_gas(
    block: &Block<Transaction>,
    gas_price_factor: u64,
) -> GasPriceResult<u64> {
    let mint = block
        .transactions()
        .last()
        .and_then(|tx| tx.as_mint())
        .ok_or(GasPriceError::CouldNotFetchL2Block {
            block_height: *block.header().height(),
            source_error: anyhow!("Block has no mint transaction"),
        })?;
    let fee = mint.mint_amount();
    let scaled_fee =
        fee.checked_mul(gas_price_factor)
            .ok_or(GasPriceError::CouldNotFetchL2Block {
                block_height: *block.header().height(),
                source_error: anyhow!(
                    "Failed to scale fee by gas price factor, overflow"
                ),
            })?;
    scaled_fee
        .checked_div(*mint.gas_price())
        .ok_or(GasPriceError::CouldNotFetchL2Block {
            block_height: *block.header().height(),
            source_error: anyhow!("Failed to calculate gas used, division by zero"),
        })
}

#[async_trait::async_trait]
impl<Database> L2BlockSource for FuelL2BlockSource<Database>
where
    Database: AtomicView,
    Database::LatestView: StorageAsRef,
    Database::LatestView: StorageInspect<FuelBlocks>,
    Database::LatestView: StorageInspect<Transactions>,
    Database::LatestView: StorageInspect<ConsensusParametersVersions>,
    <Database::LatestView as StorageInspect<FuelBlocks>>::Error: Into<anyhow::Error>,
    <Database::LatestView as StorageInspect<Transactions>>::Error: Into<anyhow::Error>,
    <Database::LatestView as StorageInspect<ConsensusParametersVersions>>::Error:
        Into<anyhow::Error>,
{
    async fn get_l2_block(&mut self, height: BlockHeight) -> GasPriceResult<BlockInfo> {
        let block = &self
            .committed_block_stream
            .next()
            .await
            .ok_or({
                GasPriceError::CouldNotFetchL2Block {
                    block_height: height,
                    source_error: anyhow!("No committed block found"),
                }
            })?
            .sealed_block
            .entity;

        let view = self.database.latest_view().map_err(|source_error| {
            GasPriceError::CouldNotFetchL2Block {
                block_height: height,
                source_error: source_error.into(),
            }
        })?;
        let param_version = block.header().consensus_parameters_version;
        let consensus_params = view
            .storage::<ConsensusParametersVersions>()
            .get(&param_version)
            .map_err(|source_error| GasPriceError::CouldNotFetchL2Block {
                block_height: height,
                source_error: source_error.into(),
            })?
            .ok_or(GasPriceError::CouldNotFetchL2Block {
                block_height: height,
                source_error: anyhow!(
                    "Consensus parameters not found in storage: {height:?}",
                ),
            })?;
        get_block_info(block, &consensus_params)
    }
}
