use crate::fuel_gas_price_updater::{
    fuel_core_storage_adapter::storage::{
        GasPriceColumn,
        GasPriceMetadata,
    },
    BlockInfo,
    Error,
    Error as GasPriceError,
    L2BlockSource,
    MetadataStorage,
    Result,
    Result as GasPriceResult,
    UpdaterMetadata,
};
use anyhow::anyhow;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    structured_storage::StructuredStorage,
    transactional::{
        Modifiable,
        WriteTransaction,
    },
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::fuel_types::BlockHeight;

use fuel_core_types::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
    fuel_tx::{
        field::{
            MintAmount,
            MintGasPrice,
        },
        Transaction,
    },
    services::block_importer::SharedImportResult,
};
use std::cmp::min;
use tokio_stream::StreamExt;

#[cfg(test)]
mod metadata_tests;

#[cfg(test)]
mod l2_source_tests;

pub mod storage;

impl<Storage> MetadataStorage for StructuredStorage<Storage>
where
    Storage: KeyValueInspect<Column = GasPriceColumn> + Modifiable,
    Storage: Send + Sync,
{
    fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> Result<Option<UpdaterMetadata>> {
        let metadata = self
            .storage::<GasPriceMetadata>()
            .get(block_height)
            .map_err(|err| Error::CouldNotFetchMetadata {
                source_error: err.into(),
            })?;
        Ok(metadata.map(|inner| inner.into_owned()))
    }

    fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()> {
        let block_height = metadata.l2_block_height();
        let mut tx = self.write_transaction();
        tx.storage_as_mut::<GasPriceMetadata>()
            .insert(&block_height, &metadata)
            .map_err(|err| Error::CouldNotSetMetadata {
                block_height,
                source_error: err.into(),
            })?;
        tx.commit().map_err(|err| Error::CouldNotSetMetadata {
            block_height,
            source_error: err.into(),
        })?;
        Ok(())
    }
}

pub struct FuelL2BlockSource<Settings> {
    genesis_block_height: BlockHeight,
    gas_price_settings: Settings,
    committed_block_stream: BoxStream<SharedImportResult>,
}

impl<Settings> FuelL2BlockSource<Settings> {
    pub fn new(
        genesis_block_height: BlockHeight,
        gas_price_settings: Settings,
        committed_block_stream: BoxStream<SharedImportResult>,
    ) -> Self {
        Self {
            genesis_block_height,
            gas_price_settings,
            committed_block_stream,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GasPriceSettings {
    pub gas_price_factor: u64,
    pub block_gas_limit: u64,
}
pub trait GasPriceSettingsProvider {
    fn settings(
        &self,
        param_version: &ConsensusParametersVersion,
    ) -> Result<GasPriceSettings>;
}

fn get_block_info(
    block: &Block<Transaction>,
    gas_price_factor: u64,
    block_gas_limit: u64,
) -> GasPriceResult<BlockInfo> {
    let (fee, gas_price) = mint_values(block)?;
    let height = *block.header().height();
    let used_gas =
        block_used_gas(height, fee, gas_price, gas_price_factor, block_gas_limit)?;
    let info = BlockInfo::Block {
        height: (*block.header().height()).into(),
        gas_used: used_gas,
        block_gas_capacity: block_gas_limit,
    };
    Ok(info)
}

fn mint_values(block: &Block<Transaction>) -> GasPriceResult<(u64, u64)> {
    let mint = block
        .transactions()
        .last()
        .and_then(|tx| tx.as_mint())
        .ok_or(GasPriceError::CouldNotFetchL2Block {
            block_height: *block.header().height(),
            source_error: anyhow!("Block has no mint transaction"),
        })?;
    Ok((*mint.mint_amount(), *mint.gas_price()))
}
fn block_used_gas(
    block_height: BlockHeight,
    fee: u64,
    gas_price: u64,
    gas_price_factor: u64,
    max_used_gas: u64,
) -> GasPriceResult<u64> {
    let scaled_fee =
        fee.checked_mul(gas_price_factor)
            .ok_or(GasPriceError::CouldNotFetchL2Block {
                block_height,
                source_error: anyhow!(
                    "Failed to scale fee by gas price factor, overflow"
                ),
            })?;
    // If gas price is zero, assume max used gas
    let approximate = scaled_fee.checked_div(gas_price).unwrap_or(max_used_gas);
    let used_gas = min(approximate, max_used_gas);
    Ok(used_gas)
}

#[async_trait::async_trait]
impl<Settings> L2BlockSource for FuelL2BlockSource<Settings>
where
    Settings: GasPriceSettingsProvider + Send + Sync,
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

        match block.header().height().cmp(&self.genesis_block_height) {
            std::cmp::Ordering::Less => Err(GasPriceError::CouldNotFetchL2Block {
                block_height: height,
                source_error: anyhow!("Block precedes expected genesis block height"),
            }),
            std::cmp::Ordering::Equal => Ok(BlockInfo::GenesisBlock),
            std::cmp::Ordering::Greater => {
                let param_version = block.header().consensus_parameters_version;

                let GasPriceSettings {
                    gas_price_factor,
                    block_gas_limit,
                } = self.gas_price_settings.settings(&param_version)?;
                get_block_info(block, gas_price_factor, block_gas_limit)
            }
        }
    }
}
