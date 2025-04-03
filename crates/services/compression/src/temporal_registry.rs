//! This module contains implementations of `TemporalRegistry` for the merkleized indexing tables
use crate::{
    ports::compression_storage::CompressionStorage as CompressionStoragePort,
    storage,
    storage::{
        evictor_cache::MetadataKey,
        timestamps::{
            TimestampKey,
            TimestampKeyspace,
        },
    },
};
use fuel_core_compression::ports::{
    EvictorDb,
    HistoryLookup,
    TemporalRegistry,
    UtxoIdToPointer,
};
use fuel_core_storage::{
    not_found,
    tables::{
        Coins,
        FuelBlocks,
        Messages,
    },
    transactional::StorageTransaction,
    Error as StorageError,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
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

/// A wrapper around a mutable reference to the compression storage
/// reused within both compression context and decompression context
pub struct CompressionStorageWrapper<'a, CS> {
    /// The mutable reference to the compression storage tx
    pub storage_tx: &'a mut StorageTransaction<CS>,
}

/// A wrapper around the compression storage, along with the
/// necessary metadata needed to perform compression
pub struct CompressionContext<'a, CS> {
    pub(crate) compression_storage: CompressionStorageWrapper<'a, CS>,
    pub(crate) block_events: &'a [Event],
}

/// A wrapper around the compression storage, along with the
/// necessary metadata needed to perform decompression
pub struct DecompressionContext<'a, CS, Onchain> {
    /// The mutable reference to the compression storage
    pub compression_storage: CompressionStorageWrapper<'a, CS>,
    /// The mutable reference to the onchain database
    pub onchain_db: Onchain,
}

macro_rules! impl_temporal_registry {
    ($type:ty) => { paste::paste! {
        impl<'a, CS> TemporalRegistry<$type> for CompressionStorageWrapper<'a, CS>
        where
            CS: CompressionStoragePort,
            for<'b> StorageTransaction<&'b mut CS>:
                StorageMutate<storage::Address, Error = StorageError>
                + StorageMutate<storage::AssetId, Error = StorageError>
                + StorageMutate<storage::ContractId, Error = StorageError>
                + StorageMutate<storage::EvictorCache, Error = StorageError>
                + StorageMutate<storage::PredicateCode, Error = StorageError>
                + StorageMutate<storage::RegistryIndex, Error = StorageError>
                + StorageMutate<storage::ScriptCode, Error = StorageError>
                + StorageMutate<storage::Timestamps, Error = StorageError>
        {
            fn read_registry(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<$type> {
                Ok(self
                    .storage_tx
                    .storage_as_ref::<crate::storage::[<$type>]>()
                    .get(key)?
                    .ok_or(not_found!(crate::storage::[<$type>]))?
                    .into_owned())
            }

            fn read_timestamp(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<Tai64> {
                let timestamp = self
                    .storage_tx
                    .storage_as_ref::<crate::storage::Timestamps>()
                    .get(&TimestampKey {
                        keyspace: TimestampKeyspace::$type,
                        key: *key,
                    })?
                    .ok_or(not_found!(crate::storage::Timestamps))?
                    .into_owned();
                Ok(timestamp)
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
                timestamp: Tai64,
            ) -> anyhow::Result<()> {
                // Write the actual value
                let old_value = self.storage_tx
                    .storage_as_mut::<crate::storage::[<$type>]>()
                    .replace(key, value)?;

                // Remove the overwritten value from index, if any
                if let Some(old_value) = old_value {
                    let old_reverse_key = (&old_value).into();
                    self.storage_tx
                        .storage_as_mut::<crate::storage::RegistryIndex>()
                        .remove(&old_reverse_key)?;
                }

                // Add the new value to the index
                let reverse_key = value.into();
                self.storage_tx
                    .storage_as_mut::<crate::storage::RegistryIndex>()
                    .insert(&reverse_key, key)?;

                // Update the timestamp
                self.storage_tx
                    .storage_as_mut::<crate::storage::Timestamps>()
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
                    .storage_tx
                    .storage_as_ref::<crate::storage::RegistryIndex>()
                    .get(&reverse_key)?
                    .map(|v| v.into_owned()))
            }
        }

        impl<'a, CS> TemporalRegistry<$type> for CompressionContext<'a, CS>
        where
            CS: CompressionStoragePort,
        {
            fn read_registry(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<$type> {
                self.compression_storage.read_registry(key)
            }

            fn read_timestamp(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<Tai64> {
                <_ as TemporalRegistry<$type>>::read_timestamp(&self.compression_storage, key)
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
                timestamp: Tai64,
            ) -> anyhow::Result<()> {
                self.compression_storage.write_registry(key, value, timestamp)
            }

            fn registry_index_lookup(
                &self,
                value: &$type,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
            {
                self.compression_storage.registry_index_lookup(value)
            }
        }

        impl<'a, CS, Offchain> TemporalRegistry<$type> for DecompressionContext<'a, CS, Offchain>
        where
            CS: CompressionStoragePort,
        {
            fn read_registry(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<$type> {
                self.compression_storage.read_registry(key)
            }

            fn read_timestamp(
                &self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<Tai64> {
                <_ as TemporalRegistry<$type>>::read_timestamp(&self.compression_storage, key)
            }

            fn write_registry(
                &mut self,
                key: &fuel_core_types::fuel_compression::RegistryKey,
                value: &$type,
                timestamp: Tai64,
            ) -> anyhow::Result<()> {
                self.compression_storage.write_registry(key, value, timestamp)
            }

            fn registry_index_lookup(
                &self,
                value: &$type,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>>
            {
                self.compression_storage.registry_index_lookup(value)
            }
        }

        impl<'a, CS> EvictorDb<$type> for CompressionContext<'a, CS>
        where
            CS: CompressionStoragePort,
        {
            fn set_latest_assigned_key(
                &mut self,
                key: fuel_core_types::fuel_compression::RegistryKey,
            ) -> anyhow::Result<()> {
                self.compression_storage.storage_tx
                    .storage_as_mut::<crate::storage::EvictorCache>()
                    .insert(&MetadataKey::$type, &key)?;
                Ok(())
            }

            fn get_latest_assigned_key(
                &self,
            ) -> anyhow::Result<Option<fuel_core_types::fuel_compression::RegistryKey>> {
                Ok(self
                    .compression_storage.storage_tx
                    .storage_as_ref::<crate::storage::EvictorCache>()
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

impl<'a, CS> UtxoIdToPointer for CompressionContext<'a, CS> {
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

impl<'a, CS, Onchain> HistoryLookup for DecompressionContext<'a, CS, Onchain>
where
    Onchain: StorageInspect<Coins, Error = fuel_core_storage::Error>
        + StorageInspect<Messages, Error = fuel_core_storage::Error>
        + StorageInspect<FuelBlocks, Error = fuel_core_storage::Error>,
{
    fn utxo_id(
        &self,
        c: fuel_core_types::fuel_tx::CompressedUtxoId,
    ) -> anyhow::Result<fuel_core_types::fuel_tx::UtxoId> {
        #[cfg(feature = "test-helpers")]
        if c.tx_pointer.block_height() == 0u32.into() {
            // This is a genesis coin, which is handled differently.
            // See CoinConfigGenerator::generate which generates the genesis coins.
            let tx_id =
                fuel_core_chain_config::coin_config_helpers::tx_id(c.output_index);

            let utxo_id = fuel_core_types::fuel_tx::UtxoId::new(tx_id, c.output_index);

            return Ok(utxo_id);
        }

        let block_info = self
            .onchain_db
            .storage_as_ref::<FuelBlocks>()
            .get(&c.tx_pointer.block_height())?
            .ok_or(not_found!(FuelBlocks))?;

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
            .get(&utxo_id)?
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

#[cfg(feature = "fault-proving")]
mod fault_proving {
    use super::*;
    use crate::storage::{
        column::CompressionColumn,
        {
            self,
        },
    };
    use fuel_core_storage::{
        blueprint::BlueprintInspect,
        kv_store::{
            KeyValueInspect,
            StorageColumn,
        },
        merkle::{
            column::MerkleizedColumn,
            sparse::{
                DummyStorage,
                Merkleized,
                MerkleizedTableColumn,
            },
        },
        structured_storage::TableWithBlueprint,
        transactional::StorageTransaction,
        Mappable,
        MerkleRoot,
        MerkleRootStorage,
    };

    trait ComputeRegistryRoot {
        fn registry_root(&self) -> crate::Result<fuel_core_types::fuel_tx::Bytes32>;
        fn root_of_table<Table>(&self) -> Result<MerkleRoot, fuel_core_storage::Error>
        where
            Table: Mappable + MerkleizedTableColumn<TableColumn = CompressionColumn>,
            Table: TableWithBlueprint,
            Table::Blueprint: BlueprintInspect<
                Table,
                DummyStorage<MerkleizedColumn<CompressionColumn>>,
            >;
    }

    impl<Storage> ComputeRegistryRoot for StorageTransaction<Storage>
    where
        Storage: KeyValueInspect<
            Column = MerkleizedColumn<storage::column::CompressionColumn>,
        >,
    {
        fn registry_root(&self) -> crate::Result<fuel_core_types::fuel_tx::Bytes32> {
            macro_rules! compute_registry_root {
                ($ty:ty) => {
                    self.root_of_table::<$ty>().map_err(
                        crate::errors::CompressionError::FailedToComputeRegistryRoot,
                    )?
                };
            }

            // don't change the order. it is important for backward compatibility.
            let roots = [
                compute_registry_root!(storage::address::Address),
                compute_registry_root!(storage::asset_id::AssetId),
                compute_registry_root!(storage::contract_id::ContractId),
                compute_registry_root!(storage::evictor_cache::EvictorCache),
                compute_registry_root!(storage::predicate_code::PredicateCode),
                compute_registry_root!(storage::script_code::ScriptCode),
                compute_registry_root!(storage::registry_index::RegistryIndex),
            ];

            let mut hasher = fuel_core_types::fuel_crypto::Hasher::default();

            for root in roots {
                hasher.input(root);
            }

            Ok(hasher.finalize())
        }

        fn root_of_table<Table>(&self) -> Result<MerkleRoot, fuel_core_storage::Error>
        where
            Table: Mappable + MerkleizedTableColumn<TableColumn = CompressionColumn>,
            Table: TableWithBlueprint,
            Table::Blueprint: BlueprintInspect<
                Table,
                DummyStorage<MerkleizedColumn<CompressionColumn>>,
            >,
        {
            <Self as MerkleRootStorage<u32, Merkleized<Table>>>::root(
                self,
                &Table::column().id(),
            )
        }
    }

    macro_rules! impl_compute_registry_root {
        ($type:ty $(, $extra_generic:ident)?) => {
            impl<'a, CS $(, $extra_generic)?> ComputeRegistryRoot for $type
            where
                CS: CompressionStoragePort,
            {
                fn registry_root(&self) -> crate::Result<fuel_core_types::fuel_tx::Bytes32> {
                    self.compression_storage.storage_tx.registry_root()
                }

                fn root_of_table<Table>(&self) -> Result<MerkleRoot, fuel_core_storage::Error>
                where
                    Table: Mappable + MerkleizedTableColumn<TableColumn = CompressionColumn>,
                    Table: TableWithBlueprint,
                    Table::Blueprint: BlueprintInspect<
                        Table,
                        DummyStorage<MerkleizedColumn<CompressionColumn>>,
                    >,
                {
                    self.compression_storage.storage_tx.root_of_table::<Table>()
                }
            }

            impl<'a, CS $(, $extra_generic)?> fuel_core_compression::ports::GetRegistryRoot for $type
            where
                Self: ComputeRegistryRoot,
            {
                type Error = crate::errors::CompressionError;

                fn registry_root(&self) -> crate::Result<fuel_core_types::fuel_tx::Bytes32> {
                    <Self as ComputeRegistryRoot>::registry_root(self)
                }
            }
        };
    }

    impl_compute_registry_root!(CompressionContext<'a, CS>);
    impl_compute_registry_root!(DecompressionContext<'a, CS, Onchain>, Onchain);
}
