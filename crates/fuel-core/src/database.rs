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
    iter::IterDirection,
    transactional::{
        StorageTransaction,
        Transactional,
    },
    Result as StorageResult,
};
use fuel_core_types::fuel_types::BlockHeight;
use serde::{
    de::DeserializeOwned,
    Serialize,
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

type DatabaseError = Error;
type DatabaseResult<T> = Result<T>;

// TODO: Extract `Database` and all belongs into `fuel-core-database`.
#[cfg(feature = "rocksdb")]
use crate::state::rocks_db::RocksDb;
#[cfg(feature = "rocksdb")]
use std::path::Path;
#[cfg(feature = "rocksdb")]
use tempfile::TempDir;

// Storages implementation
// TODO: Move to separate `database/storage` folder, because it is only implementation of storages traits.
mod block;
mod code_root;
mod contracts;
mod message;
mod receipts;
#[cfg(feature = "relayer")]
mod relayer;
mod sealed_block;
mod state;

pub(crate) mod coin;

pub mod balances;
pub mod metadata;
pub mod storage;
pub mod transaction;
pub mod transactions;
pub mod vm_database;

/// Database tables column ids to the corresponding [`fuel_core_storage::Mappable`] table.
#[repr(u32)]
#[derive(
    Copy, Clone, Debug, strum_macros::EnumCount, PartialEq, Eq, enum_iterator::Sequence,
)]
pub enum Column {
    /// The column id of metadata about the blockchain
    Metadata = 0,
    /// See [`ContractsRawCode`](fuel_core_storage::tables::ContractsRawCode)
    ContractsRawCode = 1,
    /// See [`ContractsInfo`](fuel_core_storage::tables::ContractsInfo)
    ContractsInfo = 2,
    /// See [`ContractsState`](fuel_core_storage::tables::ContractsState)
    ContractsState = 3,
    /// See [`ContractsLatestUtxo`](fuel_core_storage::tables::ContractsLatestUtxo)
    ContractsLatestUtxo = 4,
    /// See [`ContractsAssets`](fuel_core_storage::tables::ContractsAssets)
    ContractsAssets = 5,
    /// See [`Coins`](fuel_core_storage::tables::Coins)
    Coins = 6,
    /// The column of the table that stores `true` if `owner` owns `Coin` with `coin_id`
    OwnedCoins = 7,
    /// See [`Transactions`](fuel_core_storage::tables::Transactions)
    Transactions = 8,
    /// Transaction id to current status
    TransactionStatus = 9,
    /// The column of the table of all `owner`'s transactions
    TransactionsByOwnerBlockIdx = 10,
    /// See [`Receipts`](fuel_core_storage::tables::Receipts)
    Receipts = 11,
    /// See [`FuelBlocks`](fuel_core_storage::tables::FuelBlocks)
    FuelBlocks = 12,
    /// See [`FuelBlockSecondaryKeyBlockHeights`](storage::FuelBlockSecondaryKeyBlockHeights)
    FuelBlockSecondaryKeyBlockHeights = 13,
    /// See [`Messages`](fuel_core_storage::tables::Messages)
    Messages = 14,
    /// The column of the table that stores `true` if `owner` owns `Message` with `message_id`
    OwnedMessageIds = 15,
    /// See [`SealedBlockConsensus`](fuel_core_storage::tables::SealedBlockConsensus)
    FuelBlockConsensus = 16,
    /// See [`FuelBlockMerkleData`](storage::FuelBlockMerkleData)
    FuelBlockMerkleData = 17,
    /// See [`FuelBlockMerkleMetadata`](storage::FuelBlockMerkleMetadata)
    FuelBlockMerkleMetadata = 18,
    /// Messages that have been spent.
    /// Existence of a key in this column means that the message has been spent.
    /// See [`SpentMessages`](fuel_core_storage::tables::SpentMessages)
    SpentMessages = 19,
    /// Metadata for the relayer
    /// See [`RelayerMetadata`](fuel_core_relayer::ports::RelayerMetadata)
    RelayerMetadata = 20,
    /// See [`ContractsAssetsMerkleData`](storage::ContractsAssetsMerkleData)
    ContractsAssetsMerkleData = 21,
    /// See [`ContractsAssetsMerkleMetadata`](storage::ContractsAssetsMerkleMetadata)
    ContractsAssetsMerkleMetadata = 22,
    /// See [`ContractsStateMerkleData`](storage::ContractsStateMerkleData)
    ContractsStateMerkleData = 23,
    /// See [`ContractsStateMerkleMetadata`](storage::ContractsStateMerkleMetadata)
    ContractsStateMerkleMetadata = 24,
}

#[derive(Clone, Debug)]
pub struct Database {
    data: DataSource,
    // used for RAII
    _drop: Arc<DropResources>,
}

trait DropFnTrait: FnOnce() + Send + Sync {}
impl<F> DropFnTrait for F where F: FnOnce() + Send + Sync {}
type DropFn = Box<dyn DropFnTrait>;

impl fmt::Debug for DropFn {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DropFn")
    }
}

#[derive(Debug, Default)]
struct DropResources {
    // move resources into this closure to have them dropped when db drops
    drop: Option<DropFn>,
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
    #[cfg(feature = "rocksdb")]
    pub fn open(path: &Path) -> DatabaseResult<Self> {
        use anyhow::Context;
        let db = RocksDb::default_open(path).context("Failed to open rocksdb, you may need to wipe a pre-existing incompatible db `rm -rf ~/.fuel/db`")?;

        Ok(Database {
            data: Arc::new(db),
            _drop: Default::default(),
        })
    }

    pub fn in_memory() -> Self {
        Self {
            data: Arc::new(MemoryStore::default()),
            _drop: Default::default(),
        }
    }

    pub fn transaction(&self) -> DatabaseTransaction {
        self.into()
    }
}

/// Mutable methods.
// TODO: Add `&mut self` to them.
impl Database {
    fn insert<K: AsRef<[u8]>, V: Serialize, R: DeserializeOwned>(
        &self,
        key: K,
        column: Column,
        value: &V,
    ) -> DatabaseResult<Option<R>> {
        let result = self.data.put(
            key.as_ref(),
            column,
            postcard::to_stdvec(value).map_err(|_| DatabaseError::Codec)?,
        )?;
        if let Some(previous) = result {
            Ok(Some(
                postcard::from_bytes(&previous).map_err(|_| DatabaseError::Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn remove<V: DeserializeOwned>(
        &self,
        key: &[u8],
        column: Column,
    ) -> DatabaseResult<Option<V>> {
        self.data
            .delete(key, column)?
            .map(|val| postcard::from_bytes(&val).map_err(|_| DatabaseError::Codec))
            .transpose()
    }

    fn write(&self, key: &[u8], column: Column, buf: Vec<u8>) -> DatabaseResult<usize> {
        self.data.write(key, column, buf)
    }

    fn replace(
        &self,
        key: &[u8],
        column: Column,
        buf: Vec<u8>,
    ) -> DatabaseResult<(usize, Option<Vec<u8>>)> {
        self.data.replace(key, column, buf)
    }

    fn take(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        self.data.take(key, column)
    }
}

/// Read-only methods.
impl Database {
    fn contains_key(&self, key: &[u8], column: Column) -> DatabaseResult<bool> {
        self.data.exists(key, column)
    }

    fn size_of_value(&self, key: &[u8], column: Column) -> DatabaseResult<Option<usize>> {
        self.data.size_of_value(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Column,
        buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>> {
        self.data.read(key, column, buf)
    }

    fn read_alloc(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        self.data.read_alloc(key, column)
    }

    fn get<V: DeserializeOwned>(
        &self,
        key: &[u8],
        column: Column,
    ) -> DatabaseResult<Option<V>> {
        self.data
            .get(key, column)?
            .map(|val| postcard::from_bytes(&val).map_err(|_| DatabaseError::Codec))
            .transpose()
    }

    fn iter_all<K, V>(
        &self,
        column: Column,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<(K, V)>> + '_
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
    {
        self.iter_all_filtered::<K, V, Vec<u8>, Vec<u8>>(column, None, None, direction)
    }

    fn iter_all_by_prefix<K, V, P>(
        &self,
        column: Column,
        prefix: Option<P>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<(K, V)>> + '_
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
        P: AsRef<[u8]>,
    {
        self.iter_all_filtered::<K, V, P, [u8; 0]>(column, prefix, None, direction)
    }

    fn iter_all_by_start<K, V, S>(
        &self,
        column: Column,
        start: Option<S>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<(K, V)>> + '_
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
        S: AsRef<[u8]>,
    {
        self.iter_all_filtered::<K, V, [u8; 0], S>(column, None, start, direction)
    }

    fn iter_all_filtered<K, V, P, S>(
        &self,
        column: Column,
        prefix: Option<P>,
        start: Option<S>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<(K, V)>> + '_
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
        P: AsRef<[u8]>,
        S: AsRef<[u8]>,
    {
        self.data
            .iter_all(
                column,
                prefix.as_ref().map(|p| p.as_ref()),
                start.as_ref().map(|s| s.as_ref()),
                direction.unwrap_or_default(),
            )
            .map(|val| {
                val.and_then(|(key, value)| {
                    let key = K::from(key);
                    let value: V =
                        postcard::from_bytes(&value).map_err(|_| DatabaseError::Codec)?;
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

/// Construct an ephemeral database
/// uses rocksdb when rocksdb features are enabled
/// uses in-memory when rocksdb features are disabled
impl Default for Database {
    fn default() -> Self {
        #[cfg(not(feature = "rocksdb"))]
        {
            Self {
                data: Arc::new(MemoryStore::default()),
                _drop: Default::default(),
            }
        }
        #[cfg(feature = "rocksdb")]
        {
            let tmp_dir = TempDir::new().unwrap();
            Self {
                data: Arc::new(RocksDb::default_open(tmp_dir.path()).unwrap()),
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

#[cfg(feature = "rocksdb")]
pub fn convert_to_rocksdb_direction(
    direction: fuel_core_storage::iter::IterDirection,
) -> rocksdb::Direction {
    match direction {
        IterDirection::Forward => rocksdb::Direction::Forward,
        IterDirection::Reverse => rocksdb::Direction::Reverse,
    }
}
