use crate::client::schema::tx::receipt::Receipt;
use crate::client::schema::{
    schema, ConnectionArgs, ConversionError, HexString, HexString256, PageInfo,
};
use fuel_tx::bytes::Deserializable;
use std::convert::TryFrom;

pub mod receipt;

#[derive(cynic::FragmentArguments, Debug)]
pub struct TxIdArgs {
    pub id: HexString256,
}

/// Retrieves the transaction in transparent form
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
}

impl TryFrom<OpaqueTransaction> for fuel_tx::Transaction {
    type Error = ConversionError;

    fn try_from(value: OpaqueTransaction) -> Result<Self, Self::Error> {
        let bytes = value.raw_payload.0 .0;
        fuel_tx::Transaction::from_bytes(bytes.as_slice())
            .map_err(|e| ConversionError::TransactionFromBytesError(e))
    }
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Status {
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
    pub program_state: HexString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct FailureStatus {
    pub block_id: HexString256,
    pub time: super::DateTime,
    pub reason: String,
}

// mutations

#[derive(cynic::FragmentArguments)]
pub struct TxArg {
    pub tx: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "TxArg"
)]
pub struct DryRun {
    #[arguments(tx = &args.tx)]
    pub dry_run: Vec<Receipt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = TransactionQuery::build(TxIdArgs {
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
}
