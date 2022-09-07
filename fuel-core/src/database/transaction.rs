use crate::{
    database::{
        columns::{
            TRANSACTIONS,
            TRANSACTIONS_BY_OWNER_BLOCK_IDX,
            TRANSACTION_STATUS,
        },
        Database,
        KvStoreError,
    },
    model::BlockHeight,
    state::{
        Error,
        IterDirection,
    },
    tx_pool::TransactionStatus,
};
use fuel_core_interfaces::common::{
    fuel_storage::Storage,
    fuel_tx::{
        Bytes32,
        Transaction,
    },
    fuel_types::Address,
};
use std::{
    borrow::Cow,
    ops::Deref,
};

pub type TransactionIndex = u32;

impl Storage<Bytes32, Transaction> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Bytes32,
        value: &Transaction,
    ) -> Result<Option<Transaction>, KvStoreError> {
        Database::insert(self, key.as_ref(), TRANSACTIONS, value.clone())
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::remove(self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Transaction>>, KvStoreError> {
        Database::get(self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }
}

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<Transaction, Error>> + '_ {
        let start = start.map(|b| b.as_ref().to_vec());
        self.iter_all::<Vec<u8>, Transaction>(TRANSACTIONS, None, start, direction)
            .map(|res| res.map(|(_, tx)| tx))
    }

    /// Iterates over a KV mapping of `[address + block height + tx idx] => transaction id`. This
    /// allows for efficient lookup of transaction ids associated with an address, sorted by
    /// block age and ordering within a block. The cursor tracks the `[block height + tx idx]` for
    /// pagination purposes.
    pub fn owned_transactions(
        &self,
        owner: &Address,
        start: Option<&OwnedTransactionIndexCursor>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<(OwnedTransactionIndexCursor, Bytes32), Error>> + '_
    {
        let start = start
            .map(|cursor| owned_tx_index_key(owner, cursor.block_height, cursor.tx_idx));
        self.iter_all::<OwnedTransactionIndexKey, Bytes32>(
            TRANSACTIONS_BY_OWNER_BLOCK_IDX,
            Some(owner.to_vec()),
            start,
            direction,
        )
        .map(|res| res.map(|(key, tx_id)| (key.into(), tx_id)))
    }

    pub fn record_tx_id_owner(
        &self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: TransactionIndex,
        tx_id: &Bytes32,
    ) -> Result<Option<Bytes32>, Error> {
        self.insert(
            owned_tx_index_key(owner, block_height, tx_idx),
            TRANSACTIONS_BY_OWNER_BLOCK_IDX,
            *tx_id,
        )
    }

    pub fn update_tx_status(
        &self,
        tx_id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Error> {
        self.insert(tx_id.to_vec(), TRANSACTION_STATUS, status)
    }

    pub fn get_tx_status(
        &self,
        tx_id: &Bytes32,
    ) -> Result<Option<TransactionStatus>, Error> {
        self.get(&tx_id.deref()[..], TRANSACTION_STATUS)
    }
}

struct OwnedTransactionIndexKey {
    block_height: BlockHeight,
    tx_idx: TransactionIndex,
}

impl From<Vec<u8>> for OwnedTransactionIndexKey {
    fn from(bytes: Vec<u8>) -> Self {
        // the first 32 bytes are the owner, which is already known when querying
        let mut block_height_bytes: [u8; 4] = Default::default();
        block_height_bytes.copy_from_slice(&bytes[32..36]);
        let mut tx_idx_bytes: [u8; 4] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes[36..40]);

        Self {
            // owner: Address::from(owner_bytes),
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u32::from_be_bytes(tx_idx_bytes),
        }
    }
}

#[derive(Debug, PartialOrd, Eq, PartialEq)]
pub struct OwnedTransactionIndexCursor {
    pub block_height: BlockHeight,
    pub tx_idx: TransactionIndex,
}

impl From<OwnedTransactionIndexKey> for OwnedTransactionIndexCursor {
    fn from(key: OwnedTransactionIndexKey) -> Self {
        OwnedTransactionIndexCursor {
            block_height: key.block_height,
            tx_idx: key.tx_idx,
        }
    }
}

impl From<Vec<u8>> for OwnedTransactionIndexCursor {
    fn from(bytes: Vec<u8>) -> Self {
        let mut block_height_bytes: [u8; 4] = Default::default();
        block_height_bytes.copy_from_slice(&bytes[..4]);
        let mut tx_idx_bytes: [u8; 4] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes[4..8]);

        Self {
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u32::from_be_bytes(tx_idx_bytes),
        }
    }
}

impl From<OwnedTransactionIndexCursor> for Vec<u8> {
    fn from(cursor: OwnedTransactionIndexCursor) -> Self {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend(cursor.block_height.to_bytes());
        bytes.extend(cursor.tx_idx.to_be_bytes());
        bytes
    }
}

fn owned_tx_index_key(
    owner: &Address,
    height: BlockHeight,
    tx_idx: TransactionIndex,
) -> Vec<u8> {
    // generate prefix to enable sorted indexing of transactions by owner
    // owner + block_height + tx_idx
    let mut key = Vec::with_capacity(40);
    key.extend(owner.as_ref());
    key.extend(height.to_bytes());
    key.extend(tx_idx.to_be_bytes());
    key
}
