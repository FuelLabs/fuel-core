use crate::fuel_core_graphql_api::ports::DatabaseRelayedTransactions;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::relayer::transaction::RelayedTransactionStatus,
    fuel_tx::Bytes32,
};

pub fn relayed_tx_status<DB: DatabaseRelayedTransactions>(
    db: &DB,
    id: Bytes32,
) -> StorageResult<Option<RelayedTransactionStatus>> {
    db.transaction_status(id)
}
