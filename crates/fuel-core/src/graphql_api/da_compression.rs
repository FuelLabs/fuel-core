use crate::fuel_core_graphql_api::{
    ports::worker::OffChainDatabaseTransaction,
    storage::da_compression::{
        evictor_cache::MetadataKey,
        timestamps::{
            TimestampKey,
            TimestampKeyspace,
        },
        *,
    },
};
use fuel_core_compression::{
    compress::compress,
    config::Config,
    ports::{
        EvictorDb,
        HistoryLookup,
        TemporalRegistry,
        UtxoIdToPointer,
    },
};
use fuel_core_storage::{
    not_found,
    tables::{
        Coins,
        FuelBlocks,
        Messages,
    },
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
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
    tai64::Tai64,
};
use futures::FutureExt;

/// Performs DA compression for a block and stores it in the database.
pub fn da_compress_block<T>(
    config: Config,
    block: &Block,
    block_events: &[Event],
    db_tx: &mut T,
) -> anyhow::Result<()>
where
    T: OffChainDatabaseTransaction,
{
    let compressed = compress(
        config,
        CompressDbTx {
            db_tx: DbTx { db_tx },
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

pub struct DbTx<'a, Tx> {
    pub db_tx: &'a mut Tx,
}

struct CompressDbTx<'a, Tx> {
    db_tx: DbTx<'a, Tx>,
    block_events: &'a [Event],
}

pub struct DecompressDbTx<'a, Tx, Onchain> {
    pub db_tx: DbTx<'a, Tx>,
    pub onchain_db: Onchain,
}

macro_rules! impl_temporal_registry {
    ($type:ty) => { paste::paste! {
        impl<'a, Tx> TemporalRegistry<$type> for DbTx<'a, Tx>
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

            fn read_timestamp(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<Tai64> {
                Ok(self
                    .db_tx
                    .storage_as_ref::<[< DaCompressionTemporalRegistryTimestamps >]>()
                    .get(&TimestampKey {
                        keyspace: TimestampKeyspace::$type,
                        key: *key,
                    })?
                    .ok_or(not_found!(DaCompressionTemporalRegistryTimestamps))?
                    .into_owned())
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
                timestamp: Tai64,
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

                // Update the timestamp
                self.db_tx
                    .storage_as_mut::<DaCompressionTemporalRegistryTimestamps>()
                    .insert(&TimestampKey { keyspace: TimestampKeyspace::$type, key: *key }, &timestamp)?;

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

        impl<'a, Tx> TemporalRegistry<$type> for CompressTx<'a, Tx>
        where
            Tx: OffChainDatabaseTransaction,
        {
            fn read_registry(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<$type> {
                self.db_tx.read_registry(key)
            }

            fn read_timestamp(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<Tai64> {
                <_ as TemporalRegistry<$type>>::read_timestamp(&self.db_tx, key)
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
                timestamp: Tai64,
            ) -> anyhow::Result<()> {
                self.db_tx.write_registry(key, value, timestamp)
            }

            fn registry_index_lookup(
                &self,
                value: &$type,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
            {
                self.db_tx.registry_index_lookup(value)
            }
        }

        impl<'a, Tx, Offchain> TemporalRegistry<$type> for DecompressTx<'a, Tx, Offchain>
        where
            Tx: OffChainDatabaseTransaction,
        {
            fn read_registry(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<$type> {
                self.db_tx.read_registry(key)
            }

            fn read_timestamp(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<Tai64> {
                <_ as TemporalRegistry<$type>>::read_timestamp(&self.db_tx, key)
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
                timestamp: Tai64,
            ) -> anyhow::Result<()> {
                self.db_tx.write_registry(key, value, timestamp)
            }

            fn registry_index_lookup(
                &self,
                value: &$type,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
            {
                self.db_tx.registry_index_lookup(value)
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
                self.db_tx.db_tx
                    .storage_as_mut::<DaCompressionTemporalRegistryEvictorCache>()
                    .insert(&MetadataKey::$type, &key)?;
                Ok(())
            }

            fn get_latest_assigned_key(
                &self,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>> {
                Ok(self
                    .db_tx.db_tx
                    .storage_as_ref::<DaCompressionTemporalRegistryEvictorCache>()
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

impl<'a, Tx> UtxoIdToPointer for CompressDbTx<'a, Tx>
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

impl<'a, Tx, Onchain> HistoryLookup for DecompressDbTx<'a, Tx, Onchain>
where
    Tx: OffChainDatabaseTransaction,
    Onchain: StorageInspect<Coins, Error = fuel_core_storage::Error>
        + StorageInspect<Messages, Error = fuel_core_storage::Error>
        + StorageInspect<FuelBlocks, Error = fuel_core_storage::Error>,
{
    fn utxo_id(
        &self,
        c: fuel_core_types::fuel_tx::CompressedUtxoId,
    ) -> anyhow::Result<fuel_core_types::fuel_tx::UtxoId> {
        if c.tx_pointer.block_height() == 0u32.into() {
            // This is a genesis coin, which is handled differently.
            // See CoinConfigGenerator::generate which generates the genesis coins.
            let mut bytes = [0u8; 32];
            let tx_index = c.tx_pointer.tx_index();
            bytes[..std::mem::size_of_val(&tx_index)]
                .copy_from_slice(&tx_index.to_be_bytes());
            return Ok(fuel_core_types::fuel_tx::UtxoId::new(
                fuel_core_types::fuel_tx::TxId::from(bytes),
                0,
            ));
        }

        let block_info = self
            .onchain_db
            .storage_as_ref::<fuel_core_storage::tables::FuelBlocks>()
            .get(&c.tx_pointer.block_height())?
            .ok_or(not_found!(fuel_core_storage::tables::FuelBlocks))?;

        let tx_id = *block_info
            .transactions()
            .get(c.tx_pointer.tx_index() as usize)
            .ok_or(anyhow::anyhow!(
                "Transaction not found in the block: {:?}",
                c.tx_pointer
            ))?;

        Ok(fuel_core_types::fuel_tx::UtxoId::new(tx_id, c.output_index))
    }

    fn coin(
        &self,
        utxo_id: fuel_core_types::fuel_tx::UtxoId,
    ) -> anyhow::Result<fuel_core_compression::ports::CoinInfo> {
        let coin = self
            .onchain_db
            .storage_as_ref::<fuel_core_storage::tables::Coins>()
            .get(&dbg!(utxo_id))?
            .ok_or(not_found!(fuel_core_storage::tables::Coins))?;
        Ok(fuel_core_compression::ports::CoinInfo {
            owner: *coin.owner(),
            asset_id: *coin.asset_id(),
            amount: *coin.amount(),
        })
    }

    fn message(
        &self,
        nonce: fuel_core_types::fuel_types::Nonce,
    ) -> anyhow::Result<fuel_core_compression::ports::MessageInfo> {
        let message = self
            .onchain_db
            .storage_as_ref::<fuel_core_storage::tables::Messages>()
            .get(&nonce)?
            .ok_or(not_found!(fuel_core_storage::tables::Messages))?;
        Ok(fuel_core_compression::ports::MessageInfo {
            sender: *message.sender(),
            recipient: *message.recipient(),
            amount: message.amount(),
            data: message.data().clone(),
        })
    }
}
