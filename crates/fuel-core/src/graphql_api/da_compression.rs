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
    ($($name:ident: $type:ty),*$(,)?) => {
        $(
            impl<'a, Tx> TemporalRegistry<$type> for CompressTx<'a, Tx>
            where
                Tx: OffChainDatabaseTransaction,
            {
                fn read_registry(
                    &self,
                    key: fuel_core_types::fuel_compression::RegistryKey,
                ) -> anyhow::Result<$type> {
                    let v = self
                        .db_tx
                        .storage_as_ref::<DaCompressionTemporalRegistry>()
                        .get(&(RegistryKeyspace::$name, key))?
                        .ok_or(not_found!(DaCompressionTemporalRegistry))?
                        .into_owned();
                    match v {
                        RegistryKeyspaceValue::$name(v) => Ok(v),
                        _ => anyhow::bail!("Unexpected value in the registry"),
                    }
                }

                fn write_registry(
                    &mut self,
                    key: fuel_core_types::fuel_compression::RegistryKey,
                    value: $type,
                ) -> anyhow::Result<()> {
                    // Write the actual value
                    self.db_tx
                        .storage_as_mut::<DaCompressionTemporalRegistry>()
                        .insert(&(RegistryKeyspace::$name, key), &RegistryKeyspaceValue::$name(value.clone()))?;

                    // Remove the overwritten value from index, if any
                    self.db_tx
                        .storage_as_mut::<DaCompressionTemporalRegistryIndex>()
                        .remove(&RegistryKeyspaceValue::$name(value.clone()))?;

                    // Add the new value to the index
                    self.db_tx
                        .storage_as_mut::<DaCompressionTemporalRegistryIndex>()
                        .insert(&RegistryKeyspaceValue::$name(value), &key)?;

                    Ok(())
                }

                fn registry_index_lookup(
                    &self,
                    value: &$type,
                ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
                {
                    Ok(self
                        .db_tx
                        .storage_as_ref::<DaCompressionTemporalRegistryIndex>()
                        .get(&RegistryKeyspaceValue::$name(value.clone()))?
                        .map(|v| v.into_owned()))
                }
            }

        )*
    };
}

// Arguments here should match the tables! macro from crates/compression/src/tables.rs
impl_temporal_registry! {
    address: Address,
    asset_id: AssetId,
    contract_id: ContractId,
    script_code: ScriptCode,
    predicate_code: PredicateCode,
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
