use crate::{
    database::transaction::DatabaseTransaction,
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
    codec::Decode,
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
        StorageTransaction,
        Transactional,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_types::{
        BlockHeight,
        Bytes32,
        ContractId,
    },
    tai64::Tai64,
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
mod block;
mod contracts;
mod message;
mod sealed_block;
mod state;

pub(crate) mod coin;

pub mod balances;
pub mod metadata;
pub mod storage;
pub mod transaction;
pub mod transactions;

pub type Column = fuel_core_storage::column::Column;

#[derive(Clone, Debug)]
pub struct Database {
    data: StructuredStorage<DataSource>,
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

impl Database {
    pub fn new<D>(data_source: D) -> Self
    where
        D: Into<DataSource>,
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
        let db = RocksDb::default_open(path, capacity.into()).map_err(Into::<anyhow::Error>::into).context("Failed to open rocksdb, you may need to wipe a pre-existing incompatible db `rm -rf ~/.fuel/db`")?;

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
        let db = RocksDb::default_open(tmp_dir.path(), None).unwrap();
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

    pub fn transaction(&self) -> DatabaseTransaction {
        self.into()
    }

    pub fn checkpoint(&self) -> DatabaseResult<Self> {
        self.data.as_ref().checkpoint()
    }

    pub fn flush(self) -> DatabaseResult<()> {
        self.data.as_ref().flush()
    }
}

impl KeyValueStore for DataSource {
    type Column = Column;

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

impl BatchOperations for DataSource {
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = (Vec<u8>, Self::Column, WriteOperation)>,
    ) -> StorageResult<()> {
        self.as_ref().batch_write(entries)
    }
}

/// Read-only methods.
impl Database {
    fn iter_all<M>(
        &self,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint,
        M::Blueprint: Blueprint<M, DataSource>,
    {
        self.iter_all_filtered::<M, Vec<u8>, Vec<u8>>(None, None, direction)
    }

    fn iter_all_by_prefix<M, P>(
        &self,
        prefix: Option<P>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint,
        M::Blueprint: Blueprint<M, DataSource>,
        P: AsRef<[u8]>,
    {
        self.iter_all_filtered::<M, P, [u8; 0]>(prefix, None, None)
    }

    fn iter_all_by_start<M, S>(
        &self,
        start: Option<S>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint,
        M::Blueprint: Blueprint<M, DataSource>,
        S: AsRef<[u8]>,
    {
        self.iter_all_filtered::<M, [u8; 0], S>(None, start, direction)
    }

    fn iter_all_filtered<M, P, S>(
        &self,
        prefix: Option<P>,
        start: Option<S>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint,
        M::Blueprint: Blueprint<M, DataSource>,
        P: AsRef<[u8]>,
        S: AsRef<[u8]>,
    {
        self.data
            .as_ref()
            .iter_all(
                M::column(),
                prefix.as_ref().map(|p| p.as_ref()),
                start.as_ref().map(|s| s.as_ref()),
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

impl Transactional for Database {
    type Storage = Database;

    fn transaction(&self) -> StorageTransaction<Database> {
        StorageTransaction::new(self.transaction())
    }
}

impl AsRef<Database> for Database {
    fn as_ref(&self) -> &Database {
        self
    }
}

impl AsMut<Database> for Database {
    fn as_mut(&mut self) -> &mut Database {
        self
    }
}

/// Construct an ephemeral database
/// uses rocksdb when rocksdb features are enabled
/// uses in-memory when rocksdb features are disabled
impl Default for Database {
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
        Self::latest_height(self)
    }
}

impl fuel_core_storage::vm_storage::VmStorageRequirements for Database {
    type Error = StorageError;

    fn block_time(&self, height: &BlockHeight) -> StorageResult<Tai64> {
        self.block_time(height)
    }

    fn get_block_id(&self, height: &BlockHeight) -> StorageResult<Option<BlockId>> {
        self.get_block_id(height)
    }

    fn init_contract_state<S: Iterator<Item = (Bytes32, Bytes32)>>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> StorageResult<()> {
        self.init_contract_state(contract_id, slots)
    }
}

#[cfg(feature = "rocksdb")]
pub fn convert_to_rocksdb_direction(
    direction: fuel_core_storage::iter::IterDirection,
) -> rocksdb::Direction {
    match direction {
        IterDirection::Forward => rocksdb::Direction::Forward,
        IterDirection::Reverse => rocksdb::Direction::Reverse,
    }
}

#[test]
fn column_keys_not_exceed_count() {
    use enum_iterator::all;
    for column in all::<Column>() {
        assert!(column.as_usize() < Column::COUNT);
    }
}
