use super::HexString;
use crate::client::schema::{
    schema,
    U32,
};

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct StorageReadReplayEvent {
    pub column: String,
    pub key: HexString,
    pub value: Option<HexString>,
}
impl From<StorageReadReplayEvent>
    for fuel_core_types::services::executor::StorageReadReplayEvent
{
    fn from(event: StorageReadReplayEvent) -> Self {
        fuel_core_types::services::executor::StorageReadReplayEvent {
            column: event.column,
            key: event.key.into(),
            value: event.value.map(Into::into),
        }
    }
}

// mutations

#[derive(cynic::QueryVariables, Debug)]
pub struct StorageReadReplayArgs {
    pub height: U32,
}

/// Retrieves the transaction in opaque form
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    variables = "StorageReadReplayArgs"
)]
pub struct StorageReadReplay {
    #[arguments(height: $height)]
    pub storage_read_replay: Vec<Vec<StorageReadReplayEvent>>,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use fuel_core_types::fuel_types::BlockHeight;

    #[test]
    fn execution_trace_block_tx_gql_output() {
        use cynic::MutationBuilder;
        let query = StorageReadReplay::build(StorageReadReplayArgs {
            height: BlockHeight::new(1234).into(),
        });
        insta::assert_snapshot!(query.query)
    }
}
