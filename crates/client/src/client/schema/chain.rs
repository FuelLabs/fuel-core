use crate::client::schema::{
    block::Block,
    schema,
    U32,
    U64,
};
use fuel_core_types::fuel_tx::ConsensusParameters as TxConsensusParameters;

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
    pub max_storage_slots: U64,
    pub max_predicate_length: U64,
    pub max_predicate_data_length: U64,
    pub gas_price_factor: U64,
    pub gas_per_byte: U64,
    pub max_message_data_length: U64,
}

impl From<ConsensusParameters> for TxConsensusParameters {
    fn from(params: ConsensusParameters) -> Self {
        Self {
            contract_max_size: params.contract_max_size.into(),
            max_inputs: params.max_inputs.into(),
            max_outputs: params.max_outputs.into(),
            max_witnesses: params.max_witnesses.into(),
            max_gas_per_tx: params.max_gas_per_tx.into(),
            max_script_length: params.max_script_length.into(),
            max_script_data_length: params.max_script_data_length.into(),
            max_storage_slots: params.max_storage_slots.into(),
            max_predicate_length: params.max_predicate_length.into(),
            max_predicate_data_length: params.max_predicate_data_length.into(),
            gas_price_factor: params.gas_price_factor.into(),
            gas_per_byte: params.gas_per_byte.into(),
            max_message_data_length: params.max_message_data_length.into(),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct ChainQuery {
    pub chain: ChainInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChainInfo {
    pub base_chain_height: U32,
    pub name: String,
    pub peer_count: i32,
    pub latest_block: Block,
    pub consensus_parameters: ConsensusParameters,
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
