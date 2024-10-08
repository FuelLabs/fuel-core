use crate::common::utils::{
    BlockInfo,
    Error as GasPriceError,
    Result as GasPriceResult,
};
use anyhow::anyhow;
use fuel_core_types::fuel_types::BlockHeight;

use crate::{
    common::{
        fuel_core_storage_adapter::storage::{
            GasPriceColumn,
            GasPriceMetadata,
        },
        updater_metadata::UpdaterMetadata,
    },
    ports::MetadataStorage,
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
};
use std::cmp::min;

#[cfg(test)]
mod metadata_tests;

pub mod storage;

impl<Storage> MetadataStorage for StructuredStorage<Storage>
where
    Storage: KeyValueInspect<Column = GasPriceColumn> + Modifiable,
    Storage: Send + Sync,
{
    fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> GasPriceResult<Option<UpdaterMetadata>> {
        let metadata = self
            .storage::<GasPriceMetadata>()
            .get(block_height)
            .map_err(|err| GasPriceError::CouldNotFetchMetadata {
                source_error: err.into(),
            })?;
        Ok(metadata.map(|inner| inner.into_owned()))
    }

    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> GasPriceResult<()> {
        let block_height = metadata.l2_block_height();
        let mut tx = self.write_transaction();
        tx.storage_as_mut::<GasPriceMetadata>()
            .insert(&block_height, metadata)
            .and_then(|_| tx.commit())
            .map_err(|err| GasPriceError::CouldNotSetMetadata {
                block_height,
                source_error: err.into(),
            })?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GasPriceSettings {
    pub gas_price_factor: u64,
    pub block_gas_limit: u64,
}
pub trait GasPriceSettingsProvider: Send + Sync + Clone {
    fn settings(
        &self,
        param_version: &ConsensusParametersVersion,
    ) -> GasPriceResult<GasPriceSettings>;
}

pub fn get_block_info(
    block: &Block<Transaction>,
    gas_price_factor: u64,
    block_gas_limit: u64,
) -> GasPriceResult<BlockInfo> {
    let (fee, gas_price) = mint_values(block)?;
    let used_gas = block_used_gas(fee, gas_price, gas_price_factor, block_gas_limit)?;
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
            source_error: anyhow!("Block has no mint transaction"),
        })?;
    Ok((*mint.mint_amount(), *mint.gas_price()))
}
fn block_used_gas(
    fee: u64,
    gas_price: u64,
    gas_price_factor: u64,
    max_used_gas: u64,
) -> GasPriceResult<u64> {
    let scaled_fee =
        fee.checked_mul(gas_price_factor)
            .ok_or(GasPriceError::CouldNotFetchL2Block {
                source_error: anyhow!(
                    "Failed to scale fee by gas price factor, overflow"
                ),
            })?;
    // If gas price is zero, assume max used gas
    let approximate = scaled_fee.checked_div(gas_price).unwrap_or(max_used_gas);
    let used_gas = min(approximate, max_used_gas);
    Ok(used_gas)
}
