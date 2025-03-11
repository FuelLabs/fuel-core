use fuel_core_types::{
    blockchain::block::Block,
    services::{
        executor::TransactionExecutionResult,
        txpool::TransactionExecutionStatus,
    },
};

/// Converts the transaction execution result to the transaction status.
pub fn from_executor_to_status(
    block: &Block,
    result: TransactionExecutionResult,
) -> TransactionExecutionStatus {
    let time = block.header().time();
    let block_height = *block.header().height();
    match result {
        TransactionExecutionResult::Success {
            result,
            receipts,
            total_gas,
            total_fee,
        } => TransactionExecutionStatus::Success {
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
        } => TransactionExecutionStatus::Failed {
            block_height,
            time,
            result,
            receipts,
            total_gas,
            total_fee,
        },
    }
}
