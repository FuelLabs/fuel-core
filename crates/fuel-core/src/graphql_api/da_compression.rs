use crate::{
    fuel_core_graphql_api::ports::worker::OffChainDatabaseTransaction,
    graphql_api::storage::da_compression::*,
};
use fuel_core_compression::{
    compress::compress,
    ports::{
        EvictorDb,
        TemporalRegistry,
        UtxoIdToPointer,
    },
};
use fuel_core_storage::{
    not_found,
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        ContractId,
        ScriptCode,
    },
    services::executor::Event,
};
use futures::FutureExt;
use sha2::{
    Digest,
    Sha256,
};

/// Performs DA compression for a block and stores it in the database.
pub fn da_compress_block<T>(
    block: &Block,
    block_events: &[Event],
    db_tx: &mut T,
) -> anyhow::Result<()>
where
    T: OffChainDatabaseTransaction,
{
    let compressed = compress(
        CompressTx {
            db_tx,
            block_events,
        },
        block,
    )
    .now_or_never()
    .expect("The current implementation resolved all futures instantly")?;

    db_tx
        .storage_as_mut::<DaCompressedBlocks>()
        .insert(&block.header().consensus().height, &compressed)?;

    Ok(())
}

struct CompressTx<'a, Tx> {
    db_tx: &'a mut Tx,
    block_events: &'a [Event],
}

macro_rules! impl_temporal_registry {
    ($type:ty, $index_value_fn:expr) => { paste::paste! {
        impl<'a, Tx> TemporalRegistry<$type> for CompressTx<'a, Tx>
        where
            Tx: OffChainDatabaseTransaction,
        {
            fn read_registry(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<$type> {
                Ok(self
                    .db_tx
                    .storage_as_ref::<[< DaCompressionTemporalRegistry $type >]>()
                    .get(key)?
                    .ok_or(not_found!([< DaCompressionTemporalRegistry $type>]))?
                    .into_owned())
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
            ) -> anyhow::Result<()> {
                // Write the actual value
                self.db_tx
                    .storage_as_mut::<[< DaCompressionTemporalRegistry $type >]>()
                    .insert(key, value)?;

                let value_in_index: [u8; 32] = ($index_value_fn)(value);

                // Remove the overwritten value from index, if any
                self.db_tx
                    .storage_as_mut::<[< DaCompressionTemporalRegistryIndex $type >]>()
                    .remove(&value_in_index)?;

                // Add the new value to the index
                self.db_tx
                    .storage_as_mut::<[< DaCompressionTemporalRegistryIndex $type >]>()
                    .insert(&value_in_index, key)?;

                Ok(())
            }

            fn registry_index_lookup(
                &self,
                value: &$type,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
            {
                let value_in_index: [u8; 32] = ($index_value_fn)(value);
                Ok(self
                    .db_tx
                    .storage_as_ref::<[< DaCompressionTemporalRegistryIndex $type >]>()
                    .get(&value_in_index)?
                    .map(|v| v.into_owned()))
            }
        }

        impl<'a, Tx> EvictorDb<$type> for CompressTx<'a, Tx>
        where
            Tx: OffChainDatabaseTransaction,
        {
            fn write_latest(
                &mut self,
                key: fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<()> {
                self.db_tx
                    .storage_as_mut::<[< DaCompressionTemporalRegistryEvictor $type >]>()
                    .insert(&(), &key)?;
                Ok(())
            }

            fn read_latest(
                &self,
            ) -> anyhow::Result<fuel_core_types::fuel_compression::RegistryKey> {
                Ok(self
                    .db_tx
                    .storage_as_ref::<[< DaCompressionTemporalRegistryEvictor $type >]>()
                    .get(&())?
                    .ok_or(not_found!([< DaCompressionTemporalRegistryEvictor $type >]))?
                    .into_owned())
            }
        }

    }};
}

impl_temporal_registry!(Address, |v: &Address| **v);
impl_temporal_registry!(AssetId, |v: &AssetId| **v);
impl_temporal_registry!(ContractId, |v: &ContractId| **v);
impl_temporal_registry!(ScriptCode, |v: &ScriptCode| {
    let mut hasher = Sha256::new();
    hasher.update(&v.bytes);
    hasher.finalize().into()
});
impl_temporal_registry!(PredicateCode, |v: &PredicateCode| {
    let mut hasher = Sha256::new();
    hasher.update(&v.bytes);
    hasher.finalize().into()
});

impl<'a, Tx> UtxoIdToPointer for CompressTx<'a, Tx>
where
    Tx: OffChainDatabaseTransaction,
{
    fn lookup(
        &self,
        utxo_id: fuel_core_types::fuel_tx::UtxoId,
    ) -> anyhow::Result<fuel_core_types::fuel_tx::CompressedUtxoId> {
        for event in self.block_events {
            match event {
                Event::CoinCreated(coin) | Event::CoinConsumed(coin)
                    if coin.utxo_id == utxo_id =>
                {
                    let output_index = coin.utxo_id.output_index();
                    return Ok(fuel_core_types::fuel_tx::CompressedUtxoId {
                        tx_pointer: coin.tx_pointer,
                        output_index,
                    });
                }
                _ => {}
            }
        }
        anyhow::bail!("UtxoId not found in the block events");
    }
}
