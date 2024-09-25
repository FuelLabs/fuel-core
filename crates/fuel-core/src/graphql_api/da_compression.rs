use crate::{
    fuel_core_graphql_api::ports::worker::OffChainDatabaseTransaction,
    graphql_api::storage::da_compression::{
        DaCompressedBlocks,
        DaCompressionTemporalRegistry,
        DaCompressionTemporalRegistryEvictor,
        DaCompressionTemporalRegistryIndex,
    },
};
use fuel_core_compression::{
    compress::compress,
    ports::{
        EvictorDb,
        TemporalRegistry,
        UtxoIdToPointer,
    },
    RegistryKeyspace,
    RegistryKeyspaceValue,
};
use fuel_core_storage::{
    not_found,
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::block::Block,
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

impl<'a, Tx> TemporalRegistry for CompressTx<'a, Tx>
where
    Tx: OffChainDatabaseTransaction,
{
    fn read_registry(
        &self,
        keyspace: RegistryKeyspace,
        key: fuel_core_types::fuel_compression::RegistryKey,
    ) -> anyhow::Result<RegistryKeyspaceValue> {
        Ok(self
            .db_tx
            .storage_as_ref::<DaCompressionTemporalRegistry>()
            .get(&(keyspace, key))?
            .ok_or(not_found!(DaCompressionTemporalRegistry))?
            .into_owned())
    }

    fn write_registry(
        &mut self,
        key: fuel_core_types::fuel_compression::RegistryKey,
        value: RegistryKeyspaceValue,
    ) -> anyhow::Result<()> {
        // Write the actual value
        self.db_tx
            .storage_as_mut::<DaCompressionTemporalRegistry>()
            .insert(&(value.keyspace(), key), &value)?;

        // Remove the overwritten value from index, if any
        self.db_tx
            .storage_as_mut::<DaCompressionTemporalRegistryIndex>()
            .remove(&value)?;

        // Add the new value to the index
        self.db_tx
            .storage_as_mut::<DaCompressionTemporalRegistryIndex>()
            .insert(&value, &key)?;

        Ok(())
    }

    fn registry_index_lookup(
        &self,
        value: RegistryKeyspaceValue,
    ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>> {
        Ok(self
            .db_tx
            .storage_as_ref::<DaCompressionTemporalRegistryIndex>()
            .get(&value)?
            .map(|v| v.into_owned()))
    }
}

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
        panic!("UtxoId not found in the block events");
    }
}

impl<'a, Tx> EvictorDb for CompressTx<'a, Tx>
where
    Tx: OffChainDatabaseTransaction,
{
    fn write_latest(
        &mut self,
        keyspace: RegistryKeyspace,
        key: fuel_core_types::fuel_compression::RegistryKey,
    ) -> anyhow::Result<()> {
        self.db_tx
            .storage_as_mut::<DaCompressionTemporalRegistryEvictor>()
            .insert(&keyspace, &key)?;
        Ok(())
    }

    fn read_latest(
        &mut self,
        keyspace: RegistryKeyspace,
    ) -> anyhow::Result<fuel_core_types::fuel_compression::RegistryKey> {
        Ok(self
            .db_tx
            .storage_as_ref::<DaCompressionTemporalRegistryEvictor>()
            .get(&keyspace)?
            .ok_or(not_found!(DaCompressionTemporalRegistryEvictor))?
            .into_owned())
    }
}
