use crate::{
    common::{
        fuel_core_storage_adapter::storage::{
            GasPriceColumn,
            GasPriceMetadata,
            RecordedHeights,
        },
        updater_metadata::UpdaterMetadata,
        utils::{
            BlockInfo,
            Error as GasPriceError,
            Result as GasPriceResult,
        },
    },
    ports::{
        GasPriceServiceAtomicStorage,
        GetLatestRecordedHeight,
        GetMetadataStorage,
        SetLatestRecordedHeight,
        SetMetadataStorage,
    },
};
use anyhow::anyhow;
use core::cmp::min;
use fuel_core_storage::{
    codec::{
        postcard::Postcard,
        Encode,
    },
    kv_store::KeyValueInspect,
    transactional::{
        Modifiable,
        StorageTransaction,
        WriteTransaction,
    },
    Error as StorageError,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
    fuel_merkle::storage::StorageMutate,
    fuel_tx::{
        field::{
            MintAmount,
            MintGasPrice,
        },
        Transaction,
    },
    fuel_types::BlockHeight,
};

#[cfg(test)]
mod metadata_tests;

pub mod storage;

impl<Storage> SetMetadataStorage for Storage
where
    Storage: Send + Sync,
    Storage: Modifiable,
    for<'a> StorageTransaction<&'a mut Storage>:
        StorageMutate<GasPriceMetadata, Error = StorageError>,
{
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

impl<Storage> GetMetadataStorage for Storage
where
    Storage: Send + Sync,
    Storage: StorageInspect<GasPriceMetadata, Error = StorageError>,
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
}

impl<Storage> GetLatestRecordedHeight for Storage
where
    Storage: Send + Sync,
    Storage: StorageInspect<RecordedHeights, Error = StorageError>,
{
    fn get_recorded_height(&self) -> GasPriceResult<Option<BlockHeight>> {
        const KEY: &() = &();
        let recorded_height = self
            .storage::<RecordedHeights>()
            .get(KEY)
            .map_err(|err| GasPriceError::CouldNotFetchDARecord(err.into()))?
            .map(|no| *no);
        Ok(recorded_height)
    }
}

impl<Storage> GasPriceServiceAtomicStorage for Storage
where
    Storage: 'static,
    Storage: GetMetadataStorage + GetLatestRecordedHeight,
    Storage: KeyValueInspect<Column = GasPriceColumn> + Modifiable + Send + Sync + Clone,
{
    type Transaction<'a> = StorageTransaction<&'a mut Storage> where Self: 'a;

    fn begin_transaction(&mut self) -> GasPriceResult<Self::Transaction<'_>> {
        let tx = self.write_transaction();
        Ok(tx)
    }

    fn commit_transaction(transaction: Self::Transaction<'_>) -> GasPriceResult<()> {
        transaction
            .commit()
            .map_err(|err| GasPriceError::CouldNotCommit(err.into()))?;
        Ok(())
    }
}

impl<Storage> SetLatestRecordedHeight for Storage
where
    Storage: Send + Sync,
    Storage: StorageMutate<RecordedHeights, Error = StorageError>,
{
    fn set_recorded_height(
        &mut self,
        recorded_height: BlockHeight,
    ) -> GasPriceResult<()> {
        const KEY: &() = &();
        self.storage_as_mut::<RecordedHeights>()
            .insert(KEY, &recorded_height)
            .map_err(|err| GasPriceError::CouldNotFetchDARecord(err.into()))?;
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
        block_bytes: Postcard::encode(block).len() as u64,
        block_fees: fee,
        gas_price,
    };
    Ok(info)
}

pub(crate) fn mint_values(block: &Block<Transaction>) -> GasPriceResult<(u64, u64)> {
    let mint = block
        .transactions()
        .last()
        .and_then(|tx| tx.as_mint())
        .ok_or(GasPriceError::CouldNotFetchL2Block {
            source_error: anyhow!("Block has no mint transaction"),
        })?;
    Ok((*mint.mint_amount(), *mint.gas_price()))
}

// TODO: Don't take a direct dependency on `Postcard` as it's not guaranteed to be the encoding format
// https://github.com/FuelLabs/fuel-core/issues/2443
pub(crate) fn block_bytes(block: &Block<Transaction>) -> u64 {
    Postcard::encode(block).len() as u64
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
