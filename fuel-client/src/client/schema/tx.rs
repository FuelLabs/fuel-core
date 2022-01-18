use crate::client::schema::{
    schema, ConnectionArgs, ConversionError, HexString, HexString256, PageInfo,
};
use crate::client::types::TransactionResponse;
use crate::client::{PageDirection, PaginatedResult, PaginationRequest};
use fuel_types::bytes::Deserializable;
use fuel_types::Bytes32;
use std::convert::{TryFrom, TryInto};

#[derive(cynic::FragmentArguments, Debug)]
pub struct TxIdArgs {
    pub id: HexString256,
}

/// Retrieves the transaction in opaque form
#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "TxIdArgs"
)]
pub struct TransactionQuery {
    #[arguments(id = &args.id)]
    pub transaction: Option<OpaqueTransaction>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ConnectionArgs"
)]
pub struct TransactionsQuery {
    #[arguments(after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub transactions: TransactionConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TransactionConnection {
    pub edges: Option<Vec<Option<TransactionEdge>>>,
    pub page_info: PageInfo,
}

impl TryFrom<TransactionConnection> for PaginatedResult<TransactionResponse, String> {
    type Error = ConversionError;

    fn try_from(conn: TransactionConnection) -> Result<Self, Self::Error> {
        let results: Result<Vec<TransactionResponse>, Self::Error> = conn
            .edges
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| e.map(|e| e.node.try_into()))
            .collect();

        Ok(PaginatedResult {
            cursor: conn.page_info.end_cursor,
            results: results?,
        })
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TransactionEdge {
    pub cursor: String,
    pub node: OpaqueTransaction,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Transaction", schema_path = "./assets/schema.sdl")]
pub struct OpaqueTransaction {
    pub raw_payload: HexString,
    pub receipts: Option<Vec<OpaqueReceipt>>,
    pub status: Option<TransactionStatus>,
}

impl TryFrom<OpaqueTransaction> for fuel_tx::Transaction {
    type Error = ConversionError;

    fn try_from(value: OpaqueTransaction) -> Result<Self, Self::Error> {
        let bytes = value.raw_payload.0 .0;
        fuel_tx::Transaction::from_bytes(bytes.as_slice())
            .map_err(ConversionError::TransactionFromBytesError)
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Receipt", schema_path = "./assets/schema.sdl")]
pub struct OpaqueReceipt {
    pub raw_payload: HexString,
}

impl TryFrom<OpaqueReceipt> for fuel_tx::Receipt {
    type Error = ConversionError;

    fn try_from(value: OpaqueReceipt) -> Result<Self, Self::Error> {
        let bytes = value.raw_payload.0 .0;
        fuel_tx::Receipt::from_bytes(bytes.as_slice())
            .map_err(ConversionError::ReceiptFromBytesError)
    }
}

#[derive(cynic::Enum, Copy, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ReturnType {
    Return,
    ReturnData,
    Revert,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(graphql_type = "ProgramState", schema_path = "./assets/schema.sdl")]
pub struct ProgramState {
    pub return_type: ReturnType,
    pub data: HexString,
}

impl TryFrom<ProgramState> for fuel_vm::prelude::ProgramState {
    type Error = ConversionError;

    fn try_from(state: ProgramState) -> Result<Self, Self::Error> {
        Ok(match state.return_type {
            ReturnType::Return => fuel_vm::prelude::ProgramState::Return({
                let b = state.data.0 .0;
                let b: [u8; 8] = b.try_into().map_err(|_| ConversionError::BytesLength)?;
                u64::from_be_bytes(b)
            }),
            ReturnType::ReturnData => fuel_vm::prelude::ProgramState::ReturnData({
                Bytes32::try_from(state.data.0 .0.as_slice())?
            }),
            ReturnType::Revert => fuel_vm::prelude::ProgramState::Revert({
                let b = state.data.0 .0;
                let b: [u8; 8] = b.try_into().map_err(|_| ConversionError::BytesLength)?;
                u64::from_be_bytes(b)
            }),
        })
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum TransactionStatus {
    SubmittedStatus(SubmittedStatus),
    SuccessStatus(SuccessStatus),
    FailureStatus(FailureStatus),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SubmittedStatus {
    pub time: super::DateTime,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SuccessStatus {
    pub block_id: HexString256,
    pub time: super::DateTime,
    pub program_state: ProgramState,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct FailureStatus {
    pub block_id: HexString256,
    pub time: super::DateTime,
    pub reason: String,
    pub program_state: Option<ProgramState>,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct TransactionsByOwnerConnectionArgs {
    /// Select transactions based on related `owner`s
    pub owner: HexString256,
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

impl From<(HexString256, PaginationRequest<String>)> for TransactionsByOwnerConnectionArgs {
    fn from(r: (HexString256, PaginationRequest<String>)) -> Self {
        match r.1.direction {
            PageDirection::Forward => TransactionsByOwnerConnectionArgs {
                owner: r.0,
                after: r.1.cursor,
                before: None,
                first: Some(r.1.results as i32),
                last: None,
            },
            PageDirection::Backward => TransactionsByOwnerConnectionArgs {
                owner: r.0,
                after: None,
                before: r.1.cursor,
                first: None,
                last: Some(r.1.results as i32),
            },
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "TransactionsByOwnerConnectionArgs"
)]
pub struct TransactionsByOwnerQuery {
    #[arguments(owner = &args.owner, after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub transactions_by_owner: TransactionConnection,
}

// mutations

#[derive(cynic::FragmentArguments)]
pub struct TxArg {
    pub tx: HexString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "TxArg"
)]
pub struct DryRun {
    #[arguments(tx = &args.tx)]
    pub dry_run: Vec<OpaqueReceipt>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "TxArg"
)]
pub struct Submit {
    #[arguments(tx = &args.tx)]
    pub submit: HexString256,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::client::schema::Bytes;
    use fuel_types::bytes::SerializableVec;

    pub mod transparent_receipt;
    pub mod transparent_tx;

    #[test]
    fn transparent_transaction_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = transparent_tx::TransactionQuery::build(TxIdArgs {
            id: HexString256::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn opaque_transaction_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = TransactionQuery::build(TxIdArgs {
            id: HexString256::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn transactions_connection_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = TransactionsQuery::build(ConnectionArgs {
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn transactions_by_owner_gql_output() {
        use cynic::QueryBuilder;
        let operation = TransactionsByOwnerQuery::build(TransactionsByOwnerConnectionArgs {
            owner: Default::default(),
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn dry_run_tx_gql_output() {
        use cynic::MutationBuilder;
        let mut tx = fuel_tx::Transaction::default();
        let query = DryRun::build(TxArg {
            tx: HexString(Bytes(tx.to_bytes())),
        });
        insta::assert_snapshot!(query.query)
    }

    #[test]
    fn submit_tx_gql_output() {
        use cynic::MutationBuilder;
        let mut tx = fuel_tx::Transaction::default();
        let query = Submit::build(TxArg {
            tx: HexString(Bytes(tx.to_bytes())),
        });
        insta::assert_snapshot!(query.query)
    }
}
