use crate::client::schema::{block::Block, schema, U64};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ConsensusParameters {
    pub contract_max_size: U64,
    pub max_inputs: U64,
    pub max_outputs: U64,
    pub max_witnesses: U64,
    pub max_gas_per_tx: U64,
    pub max_script_length: U64,
    pub max_script_data_length: U64,
    pub max_static_contracts: U64,
    pub max_storage_slots: U64,
    pub max_predicate_length: U64,
    pub max_predicate_data_length: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct ChainQuery {
    pub chain: ChainInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChainInfo {
    pub base_chain_height: U64,
    pub name: String,
    pub peer_count: i32,
    pub latest_block: Block,
    pub transaction_parameters: ConsensusParameters,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_gql_query_output() {
        use cynic::QueryBuilder;
        let operation = ChainQuery::build(());
        insta::assert_snapshot!(operation.query)
    }
}
