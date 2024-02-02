use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            relayer::Relayer,
            DatabaseDescription,
        },
        transaction::DatabaseTransaction,
    },
    state::{
        in_memory::memory_store::MemoryStore,
        DataSource,
    },
};
use fuel_core_chain_config::{
    ChainConfigDb,
    CoinConfig,
    ContractConfig,
    MessageConfig,
};
use fuel_core_storage::{
    blueprint::Blueprint,
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    iter::IterDirection,
    kv_store::{
        BatchOperations,
        KeyValueStore,
        Value,
        WriteOperation,
    },
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    transactional::{
        AtomicView,
        StorageTransaction,
        Transactional,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use std::{
    fmt::{
        self,
        Debug,
        Formatter,
    },
    marker::Send,
    sync::Arc,
};

pub use fuel_core_database::Error;
pub type Result<T> = core::result::Result<T, Error>;

type DatabaseResult<T> = Result<T>;

// TODO: Extract `Database` and all belongs into `fuel-core-database`.
#[cfg(feature = "rocksdb")]
use crate::state::rocks_db::RocksDb;
#[cfg(feature = "rocksdb")]
use std::path::Path;
#[cfg(feature = "rocksdb")]
use tempfile::TempDir;

// Storages implementation
pub mod balances;
pub mod block;
pub mod coin;
pub mod contracts;
pub mod database_description;
pub mod message;
pub mod metadata;
pub mod sealed_block;
pub mod state;
pub mod statistic;
pub mod storage;
pub mod transaction;
pub mod transactions;

#[derive(Clone, Debug)]
pub struct Database<Description = OnChain>
where
    Description: DatabaseDescription,
{
    data: StructuredStorage<DataSource<Description>>,
    // used for RAII
    _drop: Arc<DropResources>,
}

type DropFn = Box<dyn FnOnce() + Send + Sync>;
#[derive(Default)]
struct DropResources {
    // move resources into this closure to have them dropped when db drops
    drop: Option<DropFn>,
}

impl fmt::Debug for DropResources {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DropResources")
    }
}

impl<F: 'static + FnOnce() + Send + Sync> From<F> for DropResources {
    fn from(closure: F) -> Self {
        Self {
            drop: Option::Some(Box::new(closure)),
        }
    }
}

impl Drop for DropResources {
    fn drop(&mut self) {
        if let Some(drop) = self.drop.take() {
            (drop)()
        }
    }
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
{
    pub fn new<D>(data_source: D) -> Self
    where
        D: Into<DataSource<Description>>,
    {
        Self {
            data: StructuredStorage::new(data_source.into()),
            _drop: Default::default(),
        }
    }

    pub fn with_drop(mut self, drop: DropFn) -> Self {
        self._drop = Arc::new(drop.into());
        self
    }

    #[cfg(feature = "rocksdb")]
    pub fn open(path: &Path, capacity: impl Into<Option<usize>>) -> DatabaseResult<Self> {
        use anyhow::Context;
        let db = RocksDb::<Description>::default_open(path, capacity.into()).map_err(Into::<anyhow::Error>::into).context("Failed to open rocksdb, you may need to wipe a pre-existing incompatible db `rm -rf ~/.fuel/db`")?;

        Ok(Database {
            data: StructuredStorage::new(Arc::new(db).into()),
            _drop: Default::default(),
        })
    }

    pub fn in_memory() -> Self {
        Self {
            data: StructuredStorage::new(Arc::new(MemoryStore::default()).into()),
            _drop: Default::default(),
        }
    }

    #[cfg(feature = "rocksdb")]
    pub fn rocksdb() -> Self {
        let tmp_dir = TempDir::new().unwrap();
        let db = RocksDb::<Description>::default_open(tmp_dir.path(), None).unwrap();
        Self {
            data: StructuredStorage::new(Arc::new(db).into()),
            _drop: Arc::new(
                {
                    move || {
                        // cleanup temp dir
                        drop(tmp_dir);
                    }
                }
                .into(),
            ),
        }
    }

    pub fn transaction(&self) -> DatabaseTransaction<Description> {
        self.into()
    }

    pub fn flush(self) -> DatabaseResult<()> {
        self.data.as_ref().flush()
    }
}

impl<Description> KeyValueStore for DataSource<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn put(&self, key: &[u8], column: Self::Column, value: Value) -> StorageResult<()> {
        self.as_ref().put(key, column, value)
    }

    fn replace(
        &self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        self.as_ref().replace(key, column, value)
    }

    fn write(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize> {
        self.as_ref().write(key, column, buf)
    }

    fn take(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.as_ref().take(key, column)
    }

    fn delete(&self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        self.as_ref().delete(key, column)
    }

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.as_ref().exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.as_ref().size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.as_ref().get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.as_ref().read(key, column, buf)
    }
}

impl<Description> BatchOperations for DataSource<Description>
where
    Description: DatabaseDescription,
{
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = (Vec<u8>, Self::Column, WriteOperation)>,
    ) -> StorageResult<()> {
        self.as_ref().batch_write(entries)
    }
}

/// Read-only methods.
impl<Description> Database<Description>
where
    Description: DatabaseDescription,
{
    pub(crate) fn iter_all<M>(
        &self,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: Blueprint<M, DataSource>,
    {
        self.iter_all_filtered::<M, [u8; 0]>(None, None, direction)
    }

    pub(crate) fn iter_all_by_prefix<M, P>(
        &self,
        prefix: Option<P>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: Blueprint<M, DataSource>,
        P: AsRef<[u8]>,
    {
        self.iter_all_filtered::<M, P>(prefix, None, None)
    }

    pub(crate) fn iter_all_by_start<M>(
        &self,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: Blueprint<M, DataSource>,
    {
        self.iter_all_filtered::<M, [u8; 0]>(None, start, direction)
    }

    pub(crate) fn iter_all_filtered<M, P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: Blueprint<M, DataSource>,
        P: AsRef<[u8]>,
    {
        let encoder = start.map(|start| {
            <M::Blueprint as Blueprint<M, DataSource>>::KeyCodec::encode(start)
        });

        let start = encoder.as_ref().map(|encoder| encoder.as_bytes());

        self.data
            .as_ref()
            .iter_all(
                M::column(),
                prefix.as_ref().map(|p| p.as_ref()),
                start.as_ref().map(|cow| cow.as_ref()),
                direction.unwrap_or_default(),
            )
            .map(|val| {
                val.and_then(|(key, value)| {
                    let key =
                        <M::Blueprint as Blueprint<M, DataSource>>::KeyCodec::decode(
                            key.as_slice(),
                        )
                        .map_err(|e| StorageError::Codec(anyhow::anyhow!(e)))?;
                    let value =
                        <M::Blueprint as Blueprint<M, DataSource>>::ValueCodec::decode(
                            value.as_slice(),
                        )
                        .map_err(|e| StorageError::Codec(anyhow::anyhow!(e)))?;
                    Ok((key, value))
                })
            })
    }
}

impl<Description> Transactional for Database<Description>
where
    Description: DatabaseDescription,
{
    type Storage = Database<Description>;

    fn transaction(&self) -> StorageTransaction<Database<Description>> {
        StorageTransaction::new(self.transaction())
    }
}

impl<Description> AsRef<Database<Description>> for Database<Description>
where
    Description: DatabaseDescription,
{
    fn as_ref(&self) -> &Database<Description> {
        self
    }
}

impl<Description> AsMut<Database<Description>> for Database<Description>
where
    Description: DatabaseDescription,
{
    fn as_mut(&mut self) -> &mut Database<Description> {
        self
    }
}

/// Construct an ephemeral database
/// uses rocksdb when rocksdb features are enabled
/// uses in-memory when rocksdb features are disabled
impl<Description> Default for Database<Description>
where
    Description: DatabaseDescription,
{
    fn default() -> Self {
        #[cfg(not(feature = "rocksdb"))]
        {
            Self::in_memory()
        }
        #[cfg(feature = "rocksdb")]
        {
            Self::rocksdb()
        }
    }
}

/// Implement `ChainConfigDb` so that `Database` can be passed to
/// `StateConfig's` `generate_state_config()` method
impl ChainConfigDb for Database {
    fn get_coin_config(&self) -> StorageResult<Option<Vec<CoinConfig>>> {
        Self::get_coin_config(self).map_err(Into::into)
    }

    fn get_contract_config(&self) -> StorageResult<Option<Vec<ContractConfig>>> {
        Self::get_contract_config(self)
    }

    fn get_message_config(&self) -> StorageResult<Option<Vec<MessageConfig>>> {
        Self::get_message_config(self).map_err(Into::into)
    }

    fn get_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }
}

impl AtomicView for Database<OnChain> {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> BlockHeight {
        // TODO: The database should track the latest height inside of the database object
        //  instead of fetching it from the `FuelBlocks` table. As a temporary solution,
        //  fetch it from the table for now.
        self.latest_height().unwrap_or_default()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        // TODO: Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        self.clone()
    }
}

impl AtomicView for Database<OffChain> {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> BlockHeight {
        // TODO: The database should track the latest height inside of the database object
        //  instead of fetching it from the `FuelBlocks` table. As a temporary solution,
        //  fetch it from the table for now.
        self.latest_height().unwrap_or_default()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        // TODO: Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        self.clone()
    }
}

impl AtomicView for Database<Relayer> {
    type View = Self;
    type Height = DaBlockHeight;

    fn latest_height(&self) -> Self::Height {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_relayer::ports::RelayerDb;
            // TODO: The database should track the latest da height inside of the database object
            //  instead of fetching it from the `RelayerMetadata` table. As a temporary solution,
            //  fetch it from the table for now.
            //  https://github.com/FuelLabs/fuel-core/issues/1589
            self.get_finalized_da_height().unwrap_or_default()
        }
        #[cfg(not(feature = "relayer"))]
        {
            DaBlockHeight(0)
        }
    }

    fn view_at(&self, _: &Self::Height) -> StorageResult<Self::View> {
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        self.clone()
    }
}

#[cfg(feature = "rocksdb")]
pub fn convert_to_rocksdb_direction(direction: IterDirection) -> rocksdb::Direction {
    match direction {
        IterDirection::Forward => rocksdb::Direction::Forward,
        IterDirection::Reverse => rocksdb::Direction::Reverse,
    }
}

#[cfg(test)]
mod tests {
    use crate::database::database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
        relayer::Relayer,
        DatabaseDescription,
    };

    fn column_keys_not_exceed_count<Description>()
    where
        Description: DatabaseDescription,
    {
        use enum_iterator::all;
        use fuel_core_storage::kv_store::StorageColumn;
        use strum::EnumCount;
        for column in all::<Description::Column>() {
            assert!(column.as_usize() < Description::Column::COUNT);
        }
    }

    #[test]
    fn column_keys_not_exceed_count_test_on_chain() {
        column_keys_not_exceed_count::<OnChain>();
    }

    #[test]
    fn column_keys_not_exceed_count_test_off_chain() {
        column_keys_not_exceed_count::<OffChain>();
    }

    #[test]
    fn column_keys_not_exceed_count_test_relayer() {
        column_keys_not_exceed_count::<Relayer>();
    }
}
