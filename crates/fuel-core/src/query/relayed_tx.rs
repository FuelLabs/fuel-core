use crate::{
    fuel_core_graphql_api::ports::DatabaseRelayedTransactions,
    schema::relayed_tx::RelayedTransactionStatus,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::fuel_tx::Bytes32;

pub fn relayed_tx_status<DB: DatabaseRelayedTransactions>(
    db: &DB,
    id: Bytes32,
) -> StorageResult<Option<RelayedTransactionStatus>> {
    let status = db.transaction_status(id)?;
    match status {
        Some(status) => Ok(Some(RelayedTransactionStatus(status))),
        None => Ok(None),
    }
}
