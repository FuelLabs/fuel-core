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
    pub max_gas_per_predicate: U64,
    pub gas_price_factor: U64,
    pub gas_per_byte: U64,
    pub max_message_data_length: U64,
    pub chain_id: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct GasCosts {
    pub add: U64,
    pub addi: U64,
    pub aloc: U64,
    pub and: U64,
    pub andi: U64,
    pub bal: U64,
    pub bhei: U64,
    pub bhsh: U64,
    pub burn: U64,
    pub cb: U64,
    pub cfei: U64,
    pub cfsi: U64,
    pub croo: U64,
    pub div: U64,
    pub divi: U64,
    pub ecr: U64,
    pub eq: U64,
    pub exp: U64,
    pub expi: U64,
    pub flag: U64,
    pub gm: U64,
    pub gt: U64,
    pub gtf: U64,
    pub ji: U64,
    pub jmp: U64,
    pub jne: U64,
    pub jnei: U64,
    pub jnzi: U64,
    pub jmpf: U64,
    pub jmpb: U64,
    pub jnzf: U64,
    pub jnzb: U64,
    pub jnef: U64,
    pub jneb: U64,
    pub k256: U64,
    pub lb: U64,
    pub log: U64,
    pub lt: U64,
    pub lw: U64,
    pub mcpi: U64,
    pub mint: U64,
    pub mlog: U64,
    pub mod_op: U64,
    pub modi: U64,
    pub move_op: U64,
    pub movi: U64,
    pub mroo: U64,
    pub mul: U64,
    pub muli: U64,
    pub mldv: U64,
    pub noop: U64,
    pub not: U64,
    pub or: U64,
    pub ori: U64,
    pub ret: U64,
    pub rvrt: U64,
    pub s256: U64,
    pub sb: U64,
    pub scwq: U64,
    pub sll: U64,
    pub slli: U64,
    pub srl: U64,
    pub srli: U64,
    pub srw: U64,
    pub sub: U64,
    pub subi: U64,
    pub sw: U64,
    pub sww: U64,
    pub swwq: U64,
    pub time: U64,
    pub tr: U64,
    pub tro: U64,
    pub wdcm: U64,
    pub wqcm: U64,
    pub wdop: U64,
    pub wqop: U64,
    pub wdml: U64,
    pub wqml: U64,
    pub wddv: U64,
    pub wqdv: U64,
    pub wdmd: U64,
    pub wqmd: U64,
    pub wdam: U64,
    pub wqam: U64,
    pub wdmm: U64,
    pub wqmm: U64,
    pub xor: U64,
    pub xori: U64,

    pub call: DependentCost,
    pub ccp: DependentCost,
    pub csiz: DependentCost,
    pub ldc: DependentCost,
    pub logd: DependentCost,
    pub mcl: DependentCost,
    pub mcli: DependentCost,
    pub mcp: DependentCost,
    pub meq: DependentCost,
    pub retd: DependentCost,
    pub smo: DependentCost,
    pub srwq: DependentCost,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct DependentCost {
    pub base: U64,
    pub dep_per_unit: U64,
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
            max_gas_per_predicate: params.max_gas_per_predicate.into(),
            gas_price_factor: params.gas_price_factor.into(),
            gas_per_byte: params.gas_per_byte.into(),
            max_message_data_length: params.max_message_data_length.into(),
            chain_id: params.chain_id.0.into(),
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
    pub gas_costs: GasCosts,
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
