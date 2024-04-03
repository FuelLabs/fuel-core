use crate::client::schema::{
    schema,
    RelayedTransactionId,
};

#[derive(cynic::QueryVariables, Debug)]
pub struct RelayedTransactionStatusArgs {
    /// Transaction id that contains the output message.
    pub id: RelayedTransactionId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "RelayedTransactionStatusArgs"
)]
pub struct RelayedTransactionStatusQuery {
    #[arguments(id: $id)]
    pub message_proof: Option<RelayedTransactionStatus>,
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
