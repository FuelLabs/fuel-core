use crate::{
    database::{
        transaction::{
            OwnedTransactionIndexCursor,
            OwnedTransactionIndexKey,
            TransactionIndex,
        },
        Column,
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
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::{
            Bytes32,
            Transaction,
        },
        fuel_types::Address,
    },
    db::Transactions,
};
use std::{
    borrow::Cow,
    ops::Deref,
};

impl StorageInspect<Transactions> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Transaction>>, KvStoreError> {
        Database::get(self, key.as_ref(), Column::Transaction).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), Column::Transaction).map_err(Into::into)
    }
}

impl StorageMutate<Transactions> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &Transaction,
    ) -> Result<Option<Transaction>, KvStoreError> {
        Database::insert(self, key.as_ref(), Column::Transaction, value.clone())
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::remove(self, key.as_ref(), Column::Transaction).map_err(Into::into)
    }
}

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<Transaction, Error>> + '_ {
        let start = start.map(|b| b.as_ref().to_vec());
        self.iter_all::<Vec<u8>, Transaction>(Column::Transaction, None, start, direction)
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
            Column::TransactionsByOwnerBlockIdx,
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
            Column::TransactionsByOwnerBlockIdx,
            *tx_id,
        )
    }

    pub fn update_tx_status(
        &self,
        tx_id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Error> {
        self.insert(tx_id, Column::TransactionStatus, status)
    }

    pub fn get_tx_status(
        &self,
        tx_id: &Bytes32,
    ) -> Result<Option<TransactionStatus>, Error> {
        self.get(&tx_id.deref()[..], Column::TransactionStatus)
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
