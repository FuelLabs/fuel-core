use crate::{
    database::{
        storage::UseStructuredImplementation,
        Column,
        Database,
        Error as DatabaseError,
    },
    state::DataSource,
};
use fuel_core_chain_config::ChainConfig;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    Mappable,
    Result as StorageResult,
    StorageMutate,
};

/// The table that stores all metadata. Each key is a string, while the value depends on the context.
/// The tables mostly used to store metadata for correct work of the `fuel-core`.
pub struct MetadataTable<V>(core::marker::PhantomData<V>);

impl<V> Mappable for MetadataTable<V>
where
    V: Clone,
{
    type Key = str;
    type OwnedKey = String;
    type Value = V;
    type OwnedValue = V;
}

impl<V> TableWithBlueprint for MetadataTable<V>
where
    V: Clone,
{
    type Blueprint = Plain<Postcard, Postcard>;

    fn column() -> Column {
        Column::Metadata
    }
}

impl<V> UseStructuredImplementation<MetadataTable<V>> for StructuredStorage<DataSource> where
    V: Clone
{
}

pub(crate) const DB_VERSION_KEY: &str = "version";
pub(crate) const CHAIN_NAME_KEY: &str = "chain_name";
/// Tracks the total number of transactions written to the chain
/// It's useful for analyzing TPS or other metrics.
pub(crate) const TX_COUNT: &str = "total_tx_count";

/// Can be used to perform migrations in the future.
pub(crate) const DB_VERSION: u32 = 0x00;

impl Database {
    /// Ensures the database is initialized and that the database version is correct
    pub fn init(&mut self, config: &ChainConfig) -> StorageResult<()> {
        use fuel_core_storage::StorageAsMut;
        // initialize chain name if not set
        if self.get_chain_name()?.is_none() {
            self.storage::<MetadataTable<String>>()
                .insert(CHAIN_NAME_KEY, &config.chain_name)
                .and_then(|v| {
                    if v.is_some() {
                        Err(DatabaseError::ChainAlreadyInitialized.into())
                    } else {
                        Ok(())
                    }
                })?;
        }

        // Ensure the database version is correct
        if let Some(version) = self.storage::<MetadataTable<u32>>().get(DB_VERSION_KEY)? {
            let version = version.into_owned();
            if version != DB_VERSION {
                return Err(DatabaseError::InvalidDatabaseVersion {
                    found: version,
                    expected: DB_VERSION,
                })?
            }
        } else {
            self.storage::<MetadataTable<u32>>()
                .insert(DB_VERSION_KEY, &DB_VERSION)?;
        }
        Ok(())
    }

    pub fn get_chain_name(&self) -> StorageResult<Option<String>> {
        use fuel_core_storage::StorageAsRef;
        self.storage::<MetadataTable<String>>()
            .get(CHAIN_NAME_KEY)
            .map(|v| v.map(|v| v.into_owned()))
    }

    pub fn increase_tx_count(&self, new_txs: u64) -> StorageResult<u64> {
        use fuel_core_storage::StorageAsRef;
        // TODO: how should tx count be initialized after regenesis?
        let current_tx_count: u64 = self
            .storage::<MetadataTable<u64>>()
            .get(TX_COUNT)?
            .unwrap_or_default()
            .into_owned();
        // Using saturating_add because this value doesn't significantly impact the correctness of execution.
        let new_tx_count = current_tx_count.saturating_add(new_txs);
        <_ as StorageMutate<MetadataTable<u64>>>::insert(
            // TODO: Workaround to avoid a mutable borrow of self
            &mut StructuredStorage::new(self.data.as_ref()),
            TX_COUNT,
            &new_tx_count,
        )?;
        Ok(new_tx_count)
    }

    pub fn get_tx_count(&self) -> StorageResult<u64> {
        use fuel_core_storage::StorageAsRef;
        self.storage::<MetadataTable<u64>>()
            .get(TX_COUNT)
            .map(|v| v.unwrap_or_default().into_owned())
    }
}
