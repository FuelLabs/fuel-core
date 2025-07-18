use fuel_core_executor::executor::ExecutionData;
use fuel_core_types::{
    fuel_tx::{
        MessageId,
        TxId,
    },
    services::executor::{
        Error as ExecutorError,
        Event,
        TransactionExecutionStatus,
    },
};

/// This struct is a subset of `fuel_core_executor::executor::ExecutionData` that only stores relevant details for the
/// parallel executor
#[derive(Debug)]
pub struct L1ExecutionData {
    pub coinbase: u64,
    pub used_gas: u64,
    pub used_size: u32,
    pub tx_count: u32,
    pub message_ids: Vec<MessageId>,
    pub transactions_status: Vec<TransactionExecutionStatus>,
    pub events: Vec<Event>,
    pub skipped_txs: Vec<(TxId, ExecutorError)>,
}

impl From<ExecutionData> for L1ExecutionData {
    fn from(value: ExecutionData) -> Self {
        let ExecutionData {
            coinbase,
            used_gas,
            used_size,
            tx_count,
            message_ids,
            tx_status,
            events,
            skipped_transactions,
            ..
        } = value;

        Self {
            coinbase,
            used_gas,
            used_size,
            tx_count,
            message_ids,
            transactions_status: tx_status,
            events,
            skipped_txs: skipped_transactions,
        }
    }
}
