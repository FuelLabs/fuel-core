use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    fuel_core_graphql_api::{
        ports::{
            worker,
            OffChainDatabase,
        },
        storage::transactions::OwnedTransactionIndexCursor,
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    fuel_tx::{
        Address,
        Bytes32,
        TxPointer,
    },
    fuel_types::BlockHeight,
    services::txpool::TransactionStatus,
};

impl OffChainDatabase for Database<OffChain> {
    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        self.get_tx_status(tx_id)
            .transpose()
            .ok_or(not_found!("TransactionId"))?
    }

    fn owned_transactions_ids(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>> {
        let start = start.map(|tx_pointer| OwnedTransactionIndexCursor {
            block_height: tx_pointer.block_height(),
            tx_idx: tx_pointer.tx_index(),
        });
        self.owned_transactions(owner, start, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }
}

impl worker::OffChainDatabase for Database<OffChain> {
    fn record_tx_id_owner(
        &mut self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: u16,
        tx_id: &Bytes32,
    ) -> StorageResult<Option<Bytes32>> {
        Database::record_tx_id_owner(self, owner, block_height, tx_idx, tx_id)
    }

    fn update_tx_status(
        &mut self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> StorageResult<Option<TransactionStatus>> {
        Database::update_tx_status(self, id, status)
    }

    fn increase_tx_count(&mut self, new_txs_count: u64) -> StorageResult<u64> {
        Database::increase_tx_count(self, new_txs_count)
    }
}
