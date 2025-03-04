use fuel_core_types::{
    blockchain::block::Block,
    services::{
        executor::TransactionExecutionResult,
        txpool::TransactionStatus,
    },
};

/// Converts the transaction execution result to the transaction status.
pub fn from_executor_to_status(
    block: &Block,
    result: TransactionExecutionResult,
) -> TransactionStatus {
    let time = block.header().time();
    let block_height = *block.header().height();
    match result {
        TransactionExecutionResult::Success {
            result,
            receipts,
            total_gas,
            total_fee,
        } => TransactionStatus::Success {
            block_height,
            time,
            result,
            receipts,
            total_gas,
            total_fee,
        },
        TransactionExecutionResult::Failed {
            result,
            receipts,
            total_gas,
            total_fee,
        } => TransactionStatus::Failed {
            block_height,
            time,
            result,
            receipts,
            total_gas,
            total_fee,
        },
    }
}
