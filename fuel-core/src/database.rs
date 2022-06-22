#[cfg(feature = "rocksdb")]
use crate::database::columns::COLUMN_NUM;
use crate::database::transactional::DatabaseTransaction;
use crate::model::FuelBlockDb;
#[cfg(feature = "rocksdb")]
use crate::state::rocks_db::RocksDb;
use crate::state::{
    in_memory::memory_store::MemoryStore, ColumnId, DataSource, Error, IterDirection,
};
use async_trait::async_trait;
pub use fuel_core_interfaces::db::KvStoreError;
use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_vm::prelude::{Address, Bytes32, InterpreterStorage},
    },
    model::{
        BlockHeight, ConsensusId, DaBlockHeight, SealedFuelBlock, ValidatorId, ValidatorStake,
    },
    relayer::{RelayerDb, StakingDiff},
    txpool::TxPoolDb,
};
use serde::{de::DeserializeOwned, Serialize};
#[cfg(feature = "rocksdb")]
use std::path::Path;
use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    marker::Send,
    ops::DerefMut,
    sync::Arc,
};
#[cfg(feature = "rocksdb")]
use tempfile::TempDir;

use self::columns::METADATA;

pub mod balances;
pub mod block;
pub mod code_root;
pub mod coin;
pub mod contracts;
pub mod delegates_index;
pub mod deposit_coin;
pub mod metadata;
mod receipts;
pub mod staking_diffs;
pub mod state;
pub mod transaction;
pub mod transactional;
pub mod validator_set;

// Crude way to invalidate incompatible databases,
// can be used to perform migrations in the future.
pub const VERSION: u32 = 0;

pub mod columns {
    pub const METADATA: u32 = 0;
    pub const CONTRACTS: u32 = 1;
    pub const CONTRACTS_CODE_ROOT: u32 = 2;
    pub const CONTRACTS_STATE: u32 = 3;
    // Contract Id -> Utxo Id
    pub const CONTRACT_UTXO_ID: u32 = 4;
    pub const BALANCES: u32 = 5;
    pub const COIN: u32 = 6;
    // (owner, coin id) => true
    pub const OWNED_COINS: u32 = 7;
    pub const TRANSACTIONS: u32 = 8;
    // tx id -> current status
    pub const TRANSACTION_STATUS: u32 = 9;
    pub const TRANSACTIONS_BY_OWNER_BLOCK_IDX: u32 = 10;
    pub const RECEIPTS: u32 = 11;
    pub const BLOCKS: u32 = 12;
    // maps block id -> block hash
    pub const BLOCK_IDS: u32 = 13;
    pub const TOKEN_DEPOSITS: u32 = 14;
    /// contain current validator stake and it consensus_key if set.
    pub const VALIDATOR_SET: u32 = 15;
    /// contain diff between da blocks it contains new registeres consensus key and new delegate sets.
    pub const STAKING_DIFFS: u32 = 16;
    /// Maps delegate address with validator_set_diff index where last delegate change happened
    pub const DELEGATES_INDEX: u32 = 17;

    // Number of columns
    #[cfg(feature = "rocksdb")]
    pub const COLUMN_NUM: u32 = 18;
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

/*** SAFETY: we are safe to do it because DataSource is Send+Sync and there is nowhere it is overwritten
 * it is not Send+Sync by default because Storage insert fn takes &mut self
*/
unsafe impl Send for Database {}
unsafe impl Sync for Database {}

impl TxPoolDb for Database {}

impl Database {
    #[cfg(feature = "rocksdb")]
    pub fn open(path: &Path) -> Result<Self, Error> {
        let db = RocksDb::open(path, COLUMN_NUM)?;

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

    fn insert<K: Into<Vec<u8>>, V: Serialize + DeserializeOwned>(
        &self,
        key: K,
        column: ColumnId,
        value: V,
    ) -> Result<Option<V>, Error> {
        let result = self.data.put(
            key.into(),
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
        column: ColumnId,
    ) -> Result<Option<V>, Error> {
        self.data
            .delete(key, column)?
            .map(|val| bincode::deserialize(&val).map_err(|_| Error::Codec))
            .transpose()
    }

    fn get<V: DeserializeOwned>(&self, key: &[u8], column: ColumnId) -> Result<Option<V>, Error> {
        self.data
            .get(key, column)?
            .map(|val| bincode::deserialize(&val).map_err(|_| Error::Codec))
            .transpose()
    }

    fn exists(&self, key: &[u8], column: ColumnId) -> Result<bool, Error> {
        self.data.exists(key, column)
    }

    fn iter_all<K, V>(
        &self,
        column: ColumnId,
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
            .map(|(key, value)| {
                let key = K::from(key);
                let value: V = bincode::deserialize(&value).map_err(|_| Error::Codec)?;
                Ok((key, value))
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
                data: Arc::new(RocksDb::open(tmp_dir.path(), columns::COLUMN_NUM).unwrap()),
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

    fn block_hash(&self, block_height: u32) -> Result<Bytes32, Error> {
        let hash = self.get_block_id(block_height.into())?.unwrap_or_default();
        Ok(hash)
    }

    fn coinbase(&self) -> Result<Address, Error> {
        let height = self.get_block_height()?.unwrap_or_default();
        let id = self.block_hash(height.into())?;
        let block = Storage::<Bytes32, FuelBlockDb>::get(self, &id)?.unwrap_or_default();
        Ok(block.headers.producer)
    }
}

#[async_trait]
impl RelayerDb for Database {
    async fn get_validators(&self) -> HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)> {
        struct WrapAddress(pub ValidatorId);
        impl From<Vec<u8>> for WrapAddress {
            fn from(i: Vec<u8>) -> Self {
                Self(ValidatorId::try_from(i.as_ref()).unwrap())
            }
        }
        let mut out = HashMap::new();
        for diff in self.iter_all::<WrapAddress, (ValidatorStake, Option<ConsensusId>)>(
            columns::VALIDATOR_SET,
            None,
            None,
            None,
        ) {
            match diff {
                Ok((address, stake)) => {
                    out.insert(address.0, stake);
                }
                Err(err) => panic!("Database internal error:{:?}", err),
            }
        }
        out
    }

    async fn get_staking_diffs(
        &self,
        from_da_height: DaBlockHeight,
        to_da_height: Option<DaBlockHeight>,
    ) -> Vec<(DaBlockHeight, StakingDiff)> {
        let to_da_height = if let Some(to_da_height) = to_da_height {
            if from_da_height > to_da_height {
                return Vec::new();
            }
            to_da_height
        } else {
            DaBlockHeight::MAX
        };
        struct WrapU64Be(pub DaBlockHeight);
        impl From<Vec<u8>> for WrapU64Be {
            fn from(i: Vec<u8>) -> Self {
                use byteorder::{BigEndian, ReadBytesExt};
                use std::io::Cursor;
                let mut i = Cursor::new(i);
                Self(i.read_u32::<BigEndian>().unwrap_or_default())
            }
        }
        let mut out = Vec::new();
        for diff in self.iter_all::<WrapU64Be, StakingDiff>(
            columns::STAKING_DIFFS,
            None,
            Some(from_da_height.to_be_bytes().to_vec()),
            None,
        ) {
            match diff {
                Ok((key, diff)) => {
                    let block = key.0;
                    if block > to_da_height {
                        return out;
                    }
                    out.push((block, diff))
                }
                Err(err) => panic!("get_validator_diffs unexpected error:{:?}", err),
            }
        }
        out
    }

    async fn apply_validator_diffs(
        &mut self,
        da_height: DaBlockHeight,
        changes: &HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)>,
    ) {
        // this is reimplemented here to assure it is atomic operation in case of poweroff situation.
        let mut db = self.transaction();
        // TODO
        for (address, stake) in changes {
            let _ = Storage::<ValidatorId, (ValidatorStake, Option<ConsensusId>)>::insert(
                db.deref_mut(),
                address,
                stake,
            );
        }
        db.set_validators_da_height(da_height).await;
        if let Err(err) = db.commit() {
            panic!("apply_validator_diffs database currupted: {:?}", err);
        }
    }

    async fn get_chain_height(&self) -> BlockHeight {
        match self.get_block_height() {
            Ok(res) => res.expect("get_block_height value should be always present and set"),
            Err(err) => {
                panic!("get_block_height database curruption, err:{:?}", err);
            }
        }
    }

    async fn get_sealed_block(&self, _height: BlockHeight) -> Option<Arc<SealedFuelBlock>> {
        // TODO
        Some(Arc::new(SealedFuelBlock::default()))
    }

    async fn set_finalized_da_height(&self, block: DaBlockHeight) {
        if let Err(err) = self.insert(metadata::FINALIZED_DA_HEIGHT_KEY, METADATA, block) {
            panic!("set_finalized_da_height should always succeed: {:?}", err);
        }
    }

    async fn get_finalized_da_height(&self) -> DaBlockHeight {
        match self.get(metadata::FINALIZED_DA_HEIGHT_KEY, METADATA) {
            Ok(res) => {
                return res
                    .expect("get_finalized_da_height value should be always present and set");
            }
            Err(err) => {
                panic!("get_finalized_da_height database curruption, err:{:?}", err);
            }
        }
    }

    async fn set_validators_da_height(&self, block: DaBlockHeight) {
        if let Err(err) = self.insert(metadata::VALIDATORS_DA_HEIGHT_KEY, METADATA, block) {
            panic!("set_validators_da_height should always succeed: {:?}", err);
        }
    }

    async fn get_validators_da_height(&self) -> DaBlockHeight {
        match self.get(metadata::VALIDATORS_DA_HEIGHT_KEY, METADATA) {
            Ok(res) => {
                return res
                    .expect("get_validators_da_height value should be always present and set");
            }
            Err(err) => {
                panic!(
                    "get_validators_da_height database curruption, err:{:?}",
                    err
                );
            }
        }
    }

    async fn get_last_commited_finalized_fuel_height(&self) -> BlockHeight {
        match self.get(metadata::LAST_COMMITED_FINALIZED_BLOCK_HEIGHT_KEY, METADATA) {
            Ok(res) => {
                return res
                    .expect("set_last_commited_finalized_fuel_height value should be always present and set");
            }
            Err(err) => {
                panic!(
                    "set_last_commited_finalized_fuel_height database curruption, err:{:?}",
                    err
                );
            }
        }
    }

    async fn set_last_commited_finalized_fuel_height(&self, block_height: BlockHeight) {
        if let Err(err) = self.insert(
            metadata::LAST_COMMITED_FINALIZED_BLOCK_HEIGHT_KEY,
            METADATA,
            block_height,
        ) {
            panic!(
                "set_last_commited_finalized_fuel_height should always succeed: {:?}",
                err
            );
        }
    }
}
