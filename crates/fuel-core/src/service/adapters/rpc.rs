use crate::{
    database::{
        Database,
        database_description::off_chain::OffChain,
    },
    fuel_core_graphql_api::storage::transactions::TransactionStatuses,
};
use fuel_core_block_aggregator_api::{
    blocks::importer_and_db_source::sync_service::TxReceipts,
    result::{
        Error as RPCError,
        Result as RPCResult,
    },
};
use fuel_core_storage::StorageInspect;
use fuel_core_types::{
    fuel_tx::{
        Receipt,
        TxId,
    },
    services::transaction_status::TransactionExecutionStatus,
};

pub struct ReceiptSource {
    off_chain: Database<OffChain>,
}

impl ReceiptSource {
    pub fn new(off_chain: Database<OffChain>) -> Self {
        Self { off_chain }
    }
}

impl TxReceipts for ReceiptSource {
    async fn get_receipts(&self, tx_id: &TxId) -> RPCResult<Vec<Receipt>> {
        let tx_status =
            StorageInspect::<TransactionStatuses>::get(&self.off_chain, tx_id)
                .map_err(RPCError::receipt_error)?;
        if let Some(status) = tx_status {
            match status.into_owned() {
                TransactionExecutionStatus::Success { receipts, .. } => {
                    Ok(receipts.to_vec())
                }
                _ => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests;
