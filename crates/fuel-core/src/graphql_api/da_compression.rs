use crate::fuel_core_graphql_api::{
    ports::worker::OffChainDatabaseTransaction,
    storage::da_compression::{
        metadata_key::MetadataKey,
        *,
    },
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
    ($type:ty) => { paste::paste! {
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
                let old_value = self.db_tx
                    .storage_as_mut::<[< DaCompressionTemporalRegistry $type >]>()
                    .replace(key, value)?;

                // Remove the overwritten value from index, if any
                if let Some(old_value) = old_value {
                    let old_reverse_key = (&old_value).into();
                    self.db_tx
                        .storage_as_mut::<DaCompressionTemporalRegistryIndex>()
                        .remove(&old_reverse_key)?;
                }

                // Add the new value to the index
                let reverse_key = value.into();
                self.db_tx
                    .storage_as_mut::<DaCompressionTemporalRegistryIndex>()
                    .insert(&reverse_key, key)?;

                Ok(())
            }

            fn registry_index_lookup(
                &self,
                value: &$type,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
            {
                let reverse_key = value.into();
                Ok(self
                    .db_tx
                    .storage_as_ref::<DaCompressionTemporalRegistryIndex>()
                    .get(&reverse_key)?
                    .map(|v| v.into_owned()))
            }
        }

        impl<'a, Tx> EvictorDb<$type> for CompressTx<'a, Tx>
        where
            Tx: OffChainDatabaseTransaction,
        {
            fn set_latest_assigned_key(
                &mut self,
                key: fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<()> {
                self.db_tx
                    .storage_as_mut::<DaCompressionTemporalRegistryMetadata>()
                    .insert(&MetadataKey::$type, &key)?;
                Ok(())
            }

            fn get_latest_assigned_key(
                &self,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>> {
                Ok(self
                    .db_tx
                    .storage_as_ref::<DaCompressionTemporalRegistryMetadata>()
                    .get(&MetadataKey::$type)?
                    .map(|v| v.into_owned())
                )
            }
        }

    }};
}

impl_temporal_registry!(Address);
impl_temporal_registry!(AssetId);
impl_temporal_registry!(ContractId);
impl_temporal_registry!(ScriptCode);
impl_temporal_registry!(PredicateCode);

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
