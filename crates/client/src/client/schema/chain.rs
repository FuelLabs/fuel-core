use crate::client::schema::{
    block::Block,
    schema,
    AssetId,
    U32,
    U64,
    U8,
};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ConsensusParameters {
    pub tx_params: TxParameters,
    pub predicate_params: PredicateParameters,
    pub script_params: ScriptParameters,
    pub contract_params: ContractParameters,
    pub fee_params: FeeParameters,
    pub base_asset_id: AssetId,
    pub chain_id: U64,
    pub gas_costs: GasCosts,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TxParameters {
    pub max_inputs: U8,
    pub max_outputs: U8,
    pub max_witnesses: U32,
    pub max_gas_per_tx: U64,
    pub max_size: U64,
}

impl From<TxParameters> for fuel_core_types::fuel_tx::TxParameters {
    fn from(params: TxParameters) -> Self {
        Self {
            max_inputs: params.max_inputs.into(),
            max_outputs: params.max_outputs.into(),
            max_witnesses: params.max_witnesses.into(),
            max_gas_per_tx: params.max_gas_per_tx.into(),
            max_size: params.max_size.into(),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct PredicateParameters {
    pub max_predicate_length: U64,
    pub max_predicate_data_length: U64,
    pub max_message_data_length: U64,
    pub max_gas_per_predicate: U64,
}

impl From<PredicateParameters> for fuel_core_types::fuel_tx::PredicateParameters {
    fn from(params: PredicateParameters) -> Self {
        Self {
            max_predicate_length: params.max_predicate_length.into(),
            max_predicate_data_length: params.max_predicate_data_length.into(),
            max_message_data_length: params.max_message_data_length.into(),
            max_gas_per_predicate: params.max_gas_per_predicate.into(),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ScriptParameters {
    pub max_script_length: U64,
    pub max_script_data_length: U64,
}

impl From<ScriptParameters> for fuel_core_types::fuel_tx::ScriptParameters {
    fn from(params: ScriptParameters) -> Self {
        Self {
            max_script_length: params.max_script_length.into(),
            max_script_data_length: params.max_script_data_length.into(),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractParameters {
    pub contract_max_size: U64,
    pub max_storage_slots: U64,
}

impl From<ContractParameters> for fuel_core_types::fuel_tx::ContractParameters {
    fn from(params: ContractParameters) -> Self {
        Self {
            contract_max_size: params.contract_max_size.into(),
            max_storage_slots: params.max_storage_slots.into(),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct FeeParameters {
    pub gas_price_factor: U64,
    pub gas_per_byte: U64,
}

impl From<FeeParameters> for fuel_core_types::fuel_tx::FeeParameters {
    fn from(params: FeeParameters) -> Self {
        Self {
            gas_price_factor: params.gas_price_factor.into(),
            gas_per_byte: params.gas_per_byte.into(),
        }
    }
}

macro_rules! include_from_impls_and_cynic {
    ($(#[$meta:meta])* $vis:vis struct $name:ident {
        $($field_vis:vis $field_name:ident: $field_type:ty,)*
    }) => {
        #[derive(cynic::QueryFragment, Debug)]
        #[cynic(schema_path = "./assets/schema.sdl")]
        $vis struct $name {
            $($field_vis $field_name: $field_type,)*
        }

        impl From<$name> for fuel_core_types::fuel_tx::GasCosts {
           fn from(value: $name) -> Self {
               let values = fuel_core_types::fuel_tx::GasCostsValues {
                   $($field_name: value.$field_name.into(),)*
               };
               Self::new(values)
           }
        }
    }
}

include_from_impls_and_cynic! {
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
        pub eck1: U64,
        pub ecr1: U64,
        pub ed19: U64,
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
        pub lb: U64,
        pub log: U64,
        pub lt: U64,
        pub lw: U64,
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
        pub poph: U64,
        pub popl: U64,
        pub pshh: U64,
        pub pshl: U64,
        pub ret: U64,
        pub rvrt: U64,
        pub sb: U64,
        pub sll: U64,
        pub slli: U64,
        pub srl: U64,
        pub srli: U64,
        pub srw: U64,
        pub sub: U64,
        pub subi: U64,
        pub sw: U64,
        pub sww: U64,
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
        pub k256: DependentCost,
        pub ldc: DependentCost,
        pub logd: DependentCost,
        pub mcl: DependentCost,
        pub mcli: DependentCost,
        pub mcp: DependentCost,
        pub mcpi: DependentCost,
        pub meq: DependentCost,
        pub retd: DependentCost,
        pub s256: DependentCost,
        pub scwq: DependentCost,
        pub smo: DependentCost,
        pub srwq: DependentCost,
        pub swwq: DependentCost,

        // Non-opcodes prices
        pub contract_root: DependentCost,
        pub state_root: DependentCost,
        pub vm_initialization: DependentCost,
        pub new_storage_per_byte: U64,
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct LightOperation {
    pub base: U64,
    pub units_per_gas: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct HeavyOperation {
    pub base: U64,
    pub gas_per_unit: U64,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum DependentCost {
    LightOperation(LightOperation),
    HeavyOperation(HeavyOperation),
    #[cynic(fallback)]
    Unknown,
}

impl From<DependentCost> for fuel_core_types::fuel_tx::DependentCost {
    fn from(value: DependentCost) -> Self {
        match value {
            DependentCost::LightOperation(LightOperation {
                base,
                units_per_gas,
            }) => fuel_core_types::fuel_tx::DependentCost::LightOperation {
                base: base.into(),
                units_per_gas: units_per_gas.into(),
            },
            DependentCost::HeavyOperation(HeavyOperation { base, gas_per_unit }) => {
                fuel_core_types::fuel_tx::DependentCost::HeavyOperation {
                    base: base.into(),
                    gas_per_unit: gas_per_unit.into(),
                }
            }
            _ => fuel_core_types::fuel_tx::DependentCost::HeavyOperation {
                base: 0u64,
                gas_per_unit: 0u64,
            },
        }
    }
}

impl From<ConsensusParameters> for fuel_core_types::fuel_tx::ConsensusParameters {
    fn from(params: ConsensusParameters) -> Self {
        Self {
            tx_params: params.tx_params.into(),
            predicate_params: params.predicate_params.into(),
            script_params: params.script_params.into(),
            contract_params: params.contract_params.into(),
            fee_params: params.fee_params.into(),
            base_asset_id: params.base_asset_id.into(),
            chain_id: params.chain_id.0.into(),
            gas_costs: params.gas_costs.into(),
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
    pub da_height: U64,
    pub name: String,
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
