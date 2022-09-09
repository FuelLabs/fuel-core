use crate::{
    database::{
        storage::FuelBlocks,
        transactional::DatabaseTransaction,
    },
    state::{
        in_memory::memory_store::MemoryStore,
        DataSource,
        Error,
        IterDirection,
    },
};
use async_trait::async_trait;
pub use fuel_core_interfaces::db::KvStoreError;
use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_storage::StorageAsRef,
        fuel_vm::prelude::{
            Address,
            Bytes32,
            InterpreterStorage,
        },
    },
    model::{
        BlockHeight,
        SealedFuelBlock,
    },
    p2p::P2pDb,
    relayer::RelayerDb,
    txpool::TxPoolDb,
};
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

#[cfg(feature = "rocksdb")]
use crate::state::rocks_db::RocksDb;
#[cfg(feature = "rocksdb")]
use std::path::Path;
#[cfg(feature = "rocksdb")]
use tempfile::TempDir;

pub mod metadata;
mod relayer;
pub mod storage;
pub mod transaction;
pub mod transactional;

/// Database tables column ids.
#[repr(u32)]
#[derive(
    Copy, Clone, Debug, strum_macros::EnumCount, PartialEq, Eq, enum_iterator::Sequence,
)]
pub enum Column {
    Metadata = 0,
    ContractsRawCode = 1,
    ContractsInfo = 2,
    ContractsState = 3,
    /// Contract Id -> Utxo Id
    ContractUtxoId = 4,
    Balances = 5,
    Coins = 6,
    /// (owner, coin id) => true
    OwnedCoins = 7,
    Transaction = 8,
    /// tx id -> current status
    TransactionStatus = 9,
    TransactionsByOwnerBlockIdx = 10,
    Receipts = 11,
    Blocks = 12,
    /// maps block id -> block hash
    BlockIds = 13,
    Messages = 14,
    /// Contain current validator stake and it consensus_key if set.
    ValidatorSet = 15,
    /// Contain diff between da blocks it contains new registers consensus key and new delegate sets.
    StackingDiffs = 16,
    /// Maps delegate address with validator_set_diff index where last delegate change happened.
    DelegatesIndex = 17,
    /// (Owner, MessageId) => true
    OwnedMessageIds = 18,
}

#[derive(Clone, Debug)]
pub struct Database {
    data: DataSource,
    // used for RAII
    _drop: Arc<DropResources>,
}

trait DropFnTrait: FnOnce() {}
impl<F> DropFnTrait for F where F: FnOnce() {}
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

impl<F: 'static + FnOnce()> From<F> for DropResources {
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

/// * SAFETY: we are safe to do it because DataSource is Send+Sync and there is nowhere it is overwritten
/// it is not Send+Sync by default because Storage insert fn takes &mut self
unsafe impl Send for Database {}
unsafe impl Sync for Database {}

impl Database {
    #[cfg(feature = "rocksdb")]
    pub fn open(path: &Path) -> Result<Self, Error> {
        let db = RocksDb::default_open(path)?;

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

    fn insert<K: AsRef<[u8]>, V: Serialize, R: DeserializeOwned>(
        &self,
        key: K,
        column: Column,
        value: V,
    ) -> Result<Option<R>, Error> {
        let result = self.data.put(
            key.as_ref(),
            column,
            bincode::serialize(&value).map_err(|_| Error::Codec)?,
        )?;
        if let Some(previous) = result {
            Ok(Some(
                bincode::deserialize(&previous).map_err(|_| Error::Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn remove<V: DeserializeOwned>(
        &self,
        key: &[u8],
        column: Column,
    ) -> Result<Option<V>, Error> {
        self.data
            .delete(key, column)?
            .map(|val| bincode::deserialize(&val).map_err(|_| Error::Codec))
            .transpose()
    }

    fn get<V: DeserializeOwned>(
        &self,
        key: &[u8],
        column: Column,
    ) -> Result<Option<V>, Error> {
        self.data
            .get(key, column)?
            .map(|val| bincode::deserialize(&val).map_err(|_| Error::Codec))
            .transpose()
    }

    fn exists(&self, key: &[u8], column: Column) -> Result<bool, Error> {
        self.data.exists(key, column)
    }

    fn iter_all<K, V>(
        &self,
        column: Column,
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<(K, V), Error>> + '_
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
    {
        self.data
            .iter_all(column, prefix, start, direction.unwrap_or_default())
            .map(|val| {
                val.and_then(|(key, value)| {
                    let key = K::from(key);
                    let value: V =
                        bincode::deserialize(&value).map_err(|_| Error::Codec)?;
                    Ok((key, value))
                })
            })
    }

    pub fn transaction(&self) -> DatabaseTransaction {
        self.into()
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

impl InterpreterStorage for Database {
    type DataError = Error;

    fn block_height(&self) -> Result<u32, Error> {
        let height = self.get_block_height()?.unwrap_or_default();
        Ok(height.into())
    }

    fn timestamp(&self, height: u32) -> Result<Word, Self::DataError> {
        let id = self.block_hash(height)?;
        let block = self.storage::<FuelBlocks>().get(&id)?.unwrap_or_default();
        block
            .headers
            .time
            .timestamp()
            .try_into()
            .map_err(|e| Self::DataError::DatabaseError(Box::new(e)))
    }

    fn block_hash(&self, block_height: u32) -> Result<Bytes32, Error> {
        let hash = self.get_block_id(block_height.into())?.unwrap_or_default();
        Ok(hash)
    }

    fn coinbase(&self) -> Result<Address, Error> {
        let height = self.get_block_height()?.unwrap_or_default();
        let id = self.block_hash(height.into())?;
        let block = self.storage::<FuelBlocks>().get(&id)?.unwrap_or_default();
        Ok(block.headers.producer)
    }
}

impl TxPoolDb for Database {}

#[async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> Option<Arc<SealedFuelBlock>> {
        <Self as RelayerDb>::get_sealed_block(self, height).await
    }
}
