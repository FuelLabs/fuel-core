use crate::{
    database::{
        storage::{
            FuelBlocks,
            SealedBlockConsensus,
        },
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
use fuel_block_producer::db::BlockProducerDatabase;
use fuel_chain_config::{
    ChainConfigDb,
    CoinConfig,
    ContractConfig,
};
pub use fuel_core_interfaces::db::KvStoreError;
use fuel_core_interfaces::{
    common::fuel_storage::{
        StorageAsMut,
        StorageAsRef,
    },
    model::{
        BlockHeight,
        BlockId,
        FuelBlockConsensus,
        FuelBlockDb,
        SealedFuelBlock,
    },
    not_found,
    p2p::P2pDb,
    poa_coordinator::BlockDb,
    relayer::RelayerDb,
    txpool::TxPoolDb,
};
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    borrow::Cow,
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

// Storages implementation
// TODO: Move to separate `database/storage` folder, because it is only implementation of storages traits.
mod balances;
mod block;
mod code_root;
mod coin;
mod contracts;
mod message;
mod receipts;
mod sealed_block;
mod state;

pub mod metadata;
pub mod resource;
pub mod storage;
pub mod transaction;
pub mod transactional;
pub mod vm_database;

/// Database tables column ids.
#[repr(u32)]
#[derive(
    Copy, Clone, Debug, strum_macros::EnumCount, PartialEq, Eq, enum_iterator::Sequence,
)]
pub enum Column {
    /// The column id of metadata about the blockchain
    Metadata = 0,
    /// See [`ContractsRawCode`](fuel_vm::storage::ContractsRawCode)
    ContractsRawCode = 1,
    /// See [`ContractsRawCode`](fuel_vm::storage::ContractsRawCode)
    ContractsInfo = 2,
    /// See [`ContractsState`](fuel_vm::storage::ContractsState)
    ContractsState = 3,
    /// See [`ContractsLatestUtxo`](storage::ContractsLatestUtxo)
    ContractsLatestUtxo = 4,
    /// See [`ContractsAssets`](fuel_vm::storage::ContractsAssets)
    ContractsAssets = 5,
    /// See [`Coins`](fuel_core_interfaces::db::Coins)
    Coins = 6,
    /// The column of the table that stores `true` if `owner` owns `Coin` with `coin_id`
    OwnedCoins = 7,
    /// See [`Transactions`](fuel_core_interfaces::db::Transactions)
    Transactions = 8,
    /// Transaction id to current status
    TransactionStatus = 9,
    /// The column of the table of all `owner`'s transactions
    TransactionsByOwnerBlockIdx = 10,
    /// See [`Receipts`](storage::Receipts)
    Receipts = 11,
    /// See [`FuelBlocks`](storage::FuelBlocks)
    FuelBlocks = 12,
    /// Maps fuel block id to fuel block hash
    FuelBlockIds = 13,
    /// See [`Messages`](fuel_core_interfaces::db::Messages)
    Messages = 14,
    /// The column of the table that stores `true` if `owner` owns `Message` with `message_id`
    OwnedMessageIds = 15,
    /// The column that stores the consensus metadata associated with a finalized fuel block
    FuelBlockConsensus = 16,
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

    // TODO: Get `K` and `V` by reference to force compilation error for the current
    //  code(we have many `Copy`).
    //  https://github.com/FuelLabs/fuel-core/issues/622
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

    // TODO: Rename to `contains_key` to be the same as `StorageInspect`
    //  https://github.com/FuelLabs/fuel-core/issues/622
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

impl BlockDb for Database {
    fn block_height(&self) -> anyhow::Result<BlockHeight> {
        Ok(self.get_block_height()?.unwrap_or_default())
    }

    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: FuelBlockConsensus,
    ) -> anyhow::Result<()> {
        self.storage::<SealedBlockConsensus>()
            .insert(&block_id.into(), &consensus)
            .map(|_| ())
            .map_err(Into::into)
    }
}

impl TxPoolDb for Database {
    fn current_block_height(&self) -> Result<BlockHeight, KvStoreError> {
        self.get_block_height()
            .map(|h| h.unwrap_or_default())
            .map_err(Into::into)
    }
}

impl BlockProducerDatabase for Database {
    fn get_block(
        &self,
        fuel_height: BlockHeight,
    ) -> anyhow::Result<Option<Cow<FuelBlockDb>>> {
        let id = self
            .get_block_id(fuel_height)?
            .ok_or(not_found!("BlockId"))?;
        self.storage::<FuelBlocks>().get(&id).map_err(Into::into)
    }

    fn current_block_height(&self) -> anyhow::Result<BlockHeight> {
        self.get_block_height()
            .map(|h| h.unwrap_or_default())
            .map_err(Into::into)
    }
}

#[async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> Option<Arc<SealedFuelBlock>> {
        <Self as RelayerDb>::get_sealed_block(self, height).await
    }
}

/// Implement `ChainConfigDb` so that `Database` can be passed to
/// `StateConfig's` `generate_state_config()` method
impl ChainConfigDb for Database {
    fn get_block_height(&self) -> Result<Option<BlockHeight>, Error> {
        Self::get_block_height(self)
    }

    fn get_coin_config(&self) -> anyhow::Result<Option<Vec<CoinConfig>>> {
        Self::get_coin_config(self)
    }

    fn get_contract_config(&self) -> Result<Option<Vec<ContractConfig>>, anyhow::Error> {
        Self::get_contract_config(self)
    }

    fn get_message_config(
        &self,
    ) -> Result<Option<Vec<fuel_chain_config::MessageConfig>>, Error> {
        Self::get_message_config(self)
    }
}

// TODO: Move to a separate file `database/relayer.rs`
mod relayer {
    use crate::database::{
        metadata,
        Column,
        Database,
    };
    use fuel_core_interfaces::{
        model::{
            BlockHeight,
            DaBlockHeight,
            SealedFuelBlock,
        },
        relayer::RelayerDb,
    };
    use std::sync::Arc;

    #[async_trait::async_trait]
    impl RelayerDb for Database {
        async fn get_chain_height(&self) -> BlockHeight {
            match self.get_block_height() {
                Ok(res) => {
                    res.expect("get_block_height value should be always present and set")
                }
                Err(err) => {
                    panic!("get_block_height database corruption, err:{:?}", err);
                }
            }
        }

        async fn get_sealed_block(
            &self,
            height: BlockHeight,
        ) -> Option<Arc<SealedFuelBlock>> {
            let block_id = self
                .get_block_id(height)
                .unwrap_or_else(|_| panic!("nonexistent block height {}", height))?;

            self.get_sealed_block(&block_id)
                .expect("expected to find sealed block")
                .map(Arc::new)
        }

        async fn set_finalized_da_height(&self, block: DaBlockHeight) {
            let _: Option<BlockHeight> = self
                .insert(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata, block)
                .unwrap_or_else(|err| {
                    panic!("set_finalized_da_height should always succeed: {:?}", err);
                });
        }

        async fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
            match self.get(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata) {
                Ok(res) => res,
                Err(err) => {
                    panic!("get_finalized_da_height database corruption, err:{:?}", err);
                }
            }
        }

        async fn get_last_published_fuel_height(&self) -> Option<BlockHeight> {
            match self.get(metadata::LAST_PUBLISHED_BLOCK_HEIGHT_KEY, Column::Metadata) {
                Ok(res) => res,
                Err(err) => {
                    panic!(
                    "set_last_committed_finalized_fuel_height database corruption, err:{:?}",
                    err
                );
                }
            }
        }

        async fn set_last_published_fuel_height(&self, block_height: BlockHeight) {
            if let Err(err) = self.insert::<_, _, BlockHeight>(
                metadata::LAST_PUBLISHED_BLOCK_HEIGHT_KEY,
                Column::Metadata,
                block_height,
            ) {
                panic!(
                    "set_pending_committed_fuel_height should always succeed: {:?}",
                    err
                );
            }
        }
    }
}
