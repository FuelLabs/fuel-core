use crate::database::{
    storage::DatabaseColumn,
    Column,
    Database,
    Result as DatabaseResult,
};
use fuel_core_storage::{
    iter::IterDirection,
    tables::Transactions,
};
use fuel_core_types::{
    self,
    fuel_tx::{
        Bytes32,
        Transaction,
        TxPointer,
    },
    fuel_types::{
        Address,
        BlockHeight,
    },
    services::txpool::TransactionStatus,
};
use std::{
    mem::size_of,
    ops::Deref,
};

impl DatabaseColumn for Transactions {
    fn column() -> Column {
        Column::Transactions
    }
}

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<Transaction>> + '_ {
        let start = start.map(|b| b.as_ref().to_vec());
        self.iter_all_by_start::<Vec<u8>, Transaction, _>(
            Column::Transactions,
            start,
            direction,
        )
        .map(|res| res.map(|(_, tx)| tx))
    }

    /// Iterates over a KV mapping of `[address + block height + tx idx] => transaction id`. This
    /// allows for efficient lookup of transaction ids associated with an address, sorted by
    /// block age and ordering within a block. The cursor tracks the `[block height + tx idx]` for
    /// pagination purposes.
    pub fn owned_transactions(
        &self,
        owner: Address,
        start: Option<OwnedTransactionIndexCursor>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<(TxPointer, Bytes32)>> + '_ {
        let start = start
            .map(|cursor| owned_tx_index_key(&owner, cursor.block_height, cursor.tx_idx));
        self.iter_all_filtered::<OwnedTransactionIndexKey, Bytes32, _, _>(
            Column::TransactionsByOwnerBlockIdx,
            Some(owner),
            start,
            direction,
        )
        .map(|res| {
            res.map(|(key, tx_id)| (TxPointer::new(key.block_height, key.tx_idx), tx_id))
        })
    }

    pub fn record_tx_id_owner(
        &self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: TransactionIndex,
        tx_id: &Bytes32,
    ) -> DatabaseResult<Option<Bytes32>> {
        self.insert(
            owned_tx_index_key(owner, block_height, tx_idx),
            Column::TransactionsByOwnerBlockIdx,
            tx_id,
        )
    }

    pub fn update_tx_status(
        &self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> DatabaseResult<Option<TransactionStatus>> {
        self.insert(id, Column::TransactionStatus, &status)
    }

    pub fn get_tx_status(
        &self,
        id: &Bytes32,
    ) -> DatabaseResult<Option<TransactionStatus>> {
        self.get(&id.deref()[..], Column::TransactionStatus)
    }
}

const TX_INDEX_SIZE: usize = size_of::<TransactionIndex>();
const BLOCK_HEIGHT: usize = size_of::<BlockHeight>();
const INDEX_SIZE: usize = Address::LEN + BLOCK_HEIGHT + TX_INDEX_SIZE;

fn owned_tx_index_key(
    owner: &Address,
    height: BlockHeight,
    tx_idx: TransactionIndex,
) -> [u8; INDEX_SIZE] {
    let mut default = [0u8; INDEX_SIZE];
    // generate prefix to enable sorted indexing of transactions by owner
    // owner + block_height + tx_idx
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    default[Address::LEN..Address::LEN + BLOCK_HEIGHT]
        .copy_from_slice(height.to_bytes().as_ref());
    default[Address::LEN + BLOCK_HEIGHT..].copy_from_slice(tx_idx.to_be_bytes().as_ref());
    default
}

////////////////////////////////////// Not storage part //////////////////////////////////////

pub type TransactionIndex = u16;

pub struct OwnedTransactionIndexKey {
    block_height: BlockHeight,
    tx_idx: TransactionIndex,
}

impl<T> From<T> for OwnedTransactionIndexKey
where
    T: AsRef<[u8]>,
{
    fn from(bytes: T) -> Self {
        // the first 32 bytes are the owner, which is already known when querying
        let mut block_height_bytes: [u8; 4] = Default::default();
        block_height_bytes.copy_from_slice(&bytes.as_ref()[32..36]);
        let mut tx_idx_bytes: [u8; 2] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes.as_ref()[36..38]);

        Self {
            // owner: Address::from(owner_bytes),
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u16::from_be_bytes(tx_idx_bytes),
        }
    }
}

#[derive(Clone, Debug, PartialOrd, Eq, PartialEq)]
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
        let mut tx_idx_bytes: [u8; 2] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes[4..6]);

        Self {
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u16::from_be_bytes(tx_idx_bytes),
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
