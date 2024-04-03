use crate::client::schema::{
    schema,
    Bytes32,
};

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "RelayedTransactionStatusArgs"
)]
pub struct RelayedTransactionStatusQuery {
    #[arguments(id: $ id)]
    pub status: Option<RelayedTransactionStatus>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct RelayedTransactionStatusArgs {
    /// Transaction id that contains the output message.
    pub id: Bytes32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct RelayedTransactionStatus {
    pub(crate) state: RelayedTransactionState,
}

#[derive(cynic::Enum, Copy, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum RelayedTransactionState {
    /// Transaction was included in a block, but the execution was reverted
    Failed,
}
