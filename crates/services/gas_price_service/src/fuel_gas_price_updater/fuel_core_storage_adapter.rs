use crate::fuel_gas_price_updater::{
    fuel_core_storage_adapter::database::{
        GasPriceColumn,
        GasPriceMetadata,
    },
    Error,
    MetadataStorage,
    Result,
    UpdaterMetadata,
};
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

#[cfg(test)]
mod metadata_tests;

pub mod database;

#[async_trait::async_trait]
impl<Database> MetadataStorage for StructuredStorage<Database>
where
    Database: KeyValueInspect<Column = GasPriceColumn> + Modifiable,
    Database: Send + Sync,
{
    async fn get_metadata(
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

    async fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()> {
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

use crate::fuel_gas_price_updater::{
    BlockInfo,
    Error as GasPriceError,
    L2BlockSource,
    Result as GasPriceResult,
};
use anyhow::anyhow;
use fuel_core_services::stream::BoxStream;

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
use serde::Serialize;
use std::cmp::min;
use tokio_stream::StreamExt;

#[cfg(test)]
mod l2_source_tests;

pub struct FuelL2BlockSource<Settings, Serializer> {
    gas_price_settings: Settings,
    serializer: Serializer,
    committed_block_stream: BoxStream<SharedImportResult>,
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

pub trait SerializeBytes {
    fn serialize_bytes<B: Serialize>(&self, block: B) -> u64;
}

fn get_block_info<Serializer: SerializeBytes>(
    serializer: &Serializer,
    block: &Block<Transaction>,
    gas_price_factor: u64,
    block_gas_limit: u64,
) -> GasPriceResult<BlockInfo> {
    let (fee, gas_price) = mint_values(block)?;
    let height = *block.header().height();
    let calculated_used_gas = block_used_gas(height, fee, gas_price, gas_price_factor)?;
    let used_gas = min(calculated_used_gas, block_gas_limit);
    let block_bytes = serializer.serialize_bytes(block);
    let info = BlockInfo {
        height: (*block.header().height()).into(),
        fullness: (used_gas, block_gas_limit),
        block_bytes,
        gas_price,
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
) -> GasPriceResult<u64> {
    let scaled_fee =
        fee.checked_mul(gas_price_factor)
            .ok_or(GasPriceError::CouldNotFetchL2Block {
                block_height,
                source_error: anyhow!(
                    "Failed to scale fee by gas price factor, overflow"
                ),
            })?;
    scaled_fee
        .checked_div(gas_price)
        .ok_or(GasPriceError::CouldNotFetchL2Block {
            block_height,
            source_error: anyhow!("Failed to calculate gas used, division by zero"),
        })
}

#[async_trait::async_trait]
impl<Settings, Serializer> L2BlockSource for FuelL2BlockSource<Settings, Serializer>
where
    Settings: GasPriceSettingsProvider + Send + Sync,
    Serializer: SerializeBytes + Send + Sync,
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

        let param_version = block.header().consensus_parameters_version;

        let GasPriceSettings {
            gas_price_factor,
            block_gas_limit,
        } = self.gas_price_settings.settings(&param_version)?;
        get_block_info(&self.serializer, block, gas_price_factor, block_gas_limit)
    }
}
