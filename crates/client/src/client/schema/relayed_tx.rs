use crate::client::schema::{
    schema,
    RelayedTransactionId,
    U32,
};

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "RelayedTransactionStatusArgs"
)]
pub struct RelayedTransactionStatusQuery {
    #[arguments(id: $id)]
    pub relayed_transaction_status: Option<RelayedTransactionStatus>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct RelayedTransactionStatusArgs {
    /// Transaction id that contains the output message.
    pub id: RelayedTransactionId,
}

#[allow(clippy::enum_variant_names)]
#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum RelayedTransactionStatus {
    /// Transaction was included in a block, but the execution was reverted
    Failed(RelayedTransactionFailed),
    #[cynic(fallback)]
    Unknown,
}

#[derive(cynic::QueryFragment, Clone, Debug, PartialEq, Eq)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct RelayedTransactionFailed {
    pub block_height: U32,
    pub failure: String,
}
