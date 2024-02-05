use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    fuel_core_graphql_api::storage::transactions::{
        OwnedTransactionIndexCursor,
        OwnedTransactionIndexKey,
        OwnedTransactions,
        TransactionIndex,
        TransactionStatuses,
    },
};
use fuel_core_storage::{
    iter::IterDirection,
    tables::Transactions,
    Result as StorageResult,
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

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<Transaction>> + '_ {
        self.iter_all_by_start::<Transactions>(start, direction)
            .map(|res| res.map(|(_, tx)| tx))
    }
}

impl Database<OffChain> {
    /// Iterates over a KV mapping of `[address + block height + tx idx] => transaction id`. This
    /// allows for efficient lookup of transaction ids associated with an address, sorted by
    /// block age and ordering within a block. The cursor tracks the `[block height + tx idx]` for
    /// pagination purposes.
    pub fn owned_transactions(
        &self,
        owner: Address,
        start: Option<OwnedTransactionIndexCursor>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(TxPointer, Bytes32)>> + '_ {
        let start = start.map(|cursor| {
            OwnedTransactionIndexKey::new(&owner, cursor.block_height, cursor.tx_idx)
        });
        self.iter_all_filtered::<OwnedTransactions, _>(
            Some(owner),
            start.as_ref(),
            direction,
        )
        .map(|res| {
            res.map(|(key, tx_id)| (TxPointer::new(key.block_height, key.tx_idx), tx_id))
        })
    }

    pub fn record_tx_id_owner(
        &mut self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: TransactionIndex,
        tx_id: &Bytes32,
    ) -> StorageResult<Option<Bytes32>> {
        use fuel_core_storage::StorageAsMut;
        self.storage::<OwnedTransactions>().insert(
            &OwnedTransactionIndexKey::new(owner, block_height, tx_idx),
            tx_id,
        )
    }

    pub fn update_tx_status(
        &mut self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> StorageResult<Option<TransactionStatus>> {
        use fuel_core_storage::StorageAsMut;
        self.storage::<TransactionStatuses>().insert(id, &status)
    }

    pub fn get_tx_status(
        &self,
        id: &Bytes32,
    ) -> StorageResult<Option<TransactionStatus>> {
        use fuel_core_storage::StorageAsRef;
        self.storage::<TransactionStatuses>()
            .get(id)
            .map(|v| v.map(|v| v.into_owned()))
    }
}
