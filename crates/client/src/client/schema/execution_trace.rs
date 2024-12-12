use crate::client::{
    schema::{
        schema,
        tx::transparent_receipt::Receipt,
        Address,
        ConversionError,
        TransactionId,
        U32,
        U64,
    },
    PageDirection,
    PaginationRequest,
};
use fuel_core_types::{
    fuel_tx, fuel_vm::interpreter::trace::Trigger, services::executor::{
        TransactionExecutionResult,
        TransactionExecutionStatus,
    }
};
use std::convert::{
    TryFrom,
    TryInto,
};

use super::tx::{
    ProgramState,
};

#[allow(clippy::enum_variant_names)]
#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum TraceTransactionStatus {
    SuccessStatus(TraceSuccessStatus),
    FailureStatus(TraceFailureStatus),
    #[cynic(fallback)]
    Unknown,
}

impl TryFrom<TraceTransactionStatus> for TransactionExecutionResult {
    type Error = ConversionError;

    fn try_from(status: TraceTransactionStatus) -> Result<Self, Self::Error> {
        Ok(match status {
            TraceTransactionStatus::SuccessStatus(s) => {
                let receipts = s
                    .receipts
                    .into_iter()
                    .map(|receipt| receipt.try_into())
                    .collect::<Result<Vec<fuel_tx::Receipt>, _>>()?;
                TransactionExecutionResult::Success {
                    result: s.program_state.map(TryInto::try_into).transpose()?,
                    receipts,
                    execution_trace: Vec::new(), // Not produced by dry-run
                    total_gas: s.total_gas.0,
                    total_fee: s.total_fee.0,
                }
            }
            TraceTransactionStatus::FailureStatus(s) => {
                let receipts = s
                    .receipts
                    .into_iter()
                    .map(|receipt| receipt.try_into())
                    .collect::<Result<Vec<fuel_tx::Receipt>, _>>()?;
                TransactionExecutionResult::Failed {
                    result: s.program_state.map(TryInto::try_into).transpose()?,
                    receipts,
                    execution_trace: Vec::new(), // Not produced by dry-run
                    total_gas: s.total_gas.0,
                    total_fee: s.total_fee.0,
                }
            }
            TraceTransactionStatus::Unknown => {
                return Err(Self::Error::UnknownVariant("ExecutionTraceTxStatus"))
            }
        })
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TraceSuccessStatus {
    pub program_state: Option<ProgramState>,
    pub receipts: Vec<Receipt>,
    pub total_gas: U64,
    pub total_fee: U64,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TraceFailureStatus {
    pub program_state: Option<ProgramState>,
    pub receipts: Vec<Receipt>,
    pub total_gas: U64,
    pub total_fee: U64,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TraceTransactionExecutionStatus {
    pub id: TransactionId,
    pub status: TraceTransactionStatus,
}

impl TryFrom<TraceTransactionExecutionStatus> for TransactionExecutionStatus {
    type Error = ConversionError;

    fn try_from(schema: TraceTransactionExecutionStatus) -> Result<Self, Self::Error> {
        let id = schema.id.into();
        let status = schema.status.try_into()?;

        Ok(TransactionExecutionStatus { id, result: status })
    }
}

#[derive(cynic::QueryVariables, Debug)]
pub struct TransactionsByOwnerConnectionArgs {
    /// Select transactions based on related `owner`s
    pub owner: Address,
    /// Skip until cursor (forward pagination)
    pub after: Option<String>,
    /// Skip until cursor (backward pagination)
    pub before: Option<String>,
    /// Retrieve the first n transactions in order (forward pagination)
    pub first: Option<i32>,
    /// Retrieve the last n transactions in order (backward pagination).
    /// Can't be used at the same time as `first`.
    pub last: Option<i32>,
}

impl From<(Address, PaginationRequest<String>)> for TransactionsByOwnerConnectionArgs {
    fn from(r: (Address, PaginationRequest<String>)) -> Self {
        match r.1.direction {
            PageDirection::Forward => TransactionsByOwnerConnectionArgs {
                owner: r.0,
                after: r.1.cursor,
                before: None,
                first: Some(r.1.results),
                last: None,
            },
            PageDirection::Backward => TransactionsByOwnerConnectionArgs {
                owner: r.0,
                after: None,
                before: r.1.cursor,
                first: None,
                last: Some(r.1.results),
            },
        }
    }
}

// mutations


/// When to record a trace frames during execution
#[derive(cynic::Enum, Debug, Copy, Clone, Eq, PartialEq)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "TraceTrigger",
)]
pub enum TraceTrigger {
    /// After each instruction
    OnInstruction,
    /// After an instruction has created a receipt
    OnReceipt,
}
impl From<Trigger> for TraceTrigger {
    fn from(value: Trigger) -> Self {
        match value {
            Trigger::OnInstruction => TraceTrigger::OnInstruction,
            Trigger::OnReceipt => TraceTrigger::OnReceipt,
        }
    }
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ExectionTraceBlockArgs {
    pub height: U32,
    pub trigger: TraceTrigger,
}

/// Retrieves the transaction in opaque form
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    variables = "ExectionTraceBlockArgs"
)]
pub struct ExectionTraceBlock {
    #[arguments(height: $height, trigger: $trigger)]
    pub execution_trace_block: Vec<TraceTransactionExecutionStatus>,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use fuel_core_types::fuel_types::BlockHeight;

    #[test]
    fn execution_trace_block_tx_gql_output() {
        use cynic::MutationBuilder;
        let query = ExectionTraceBlock::build(ExectionTraceBlockArgs {
            height: BlockHeight::new(1234).into(),
            trigger: TraceTrigger::OnReceipt,
        });
        insta::assert_snapshot!(query.query)
    }
}
