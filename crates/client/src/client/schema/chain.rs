use crate::client::schema::{
    block::Block,
    schema,
    U32,
    U64,
};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ConsensusParameters {
    pub tx_params: TxParameters,
    pub predicate_params: PredicateParameters,
    pub script_params: ScriptParameters,
    pub contract_params: ContractParameters,
    pub fee_params: FeeParameters,
    pub chain_id: U64,
    pub gas_costs: GasCosts,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TxParameters {
    pub max_inputs: U64,
    pub max_outputs: U64,
    pub max_witnesses: U64,
    pub max_gas_per_tx: U64,
}

impl From<TxParameters> for fuel_core_types::fuel_tx::TxParameters {
    fn from(params: TxParameters) -> Self {
        Self {
            max_inputs: params.max_inputs.0,
            max_outputs: params.max_outputs.0,
            max_witnesses: params.max_witnesses.0,
            max_gas_per_tx: params.max_gas_per_tx.0,
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
            max_predicate_length: params.max_predicate_length.0,
            max_predicate_data_length: params.max_predicate_data_length.0,
            max_message_data_length: params.max_message_data_length.0,
            max_gas_per_predicate: params.max_gas_per_predicate.0,
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
            max_script_length: params.max_script_length.0,
            max_script_data_length: params.max_script_data_length.0,
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
            contract_max_size: params.contract_max_size.0,
            max_storage_slots: params.max_storage_slots.0,
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
            gas_price_factor: params.gas_price_factor.0,
            gas_per_byte: params.gas_per_byte.0,
        }
    }
}

// macro_rules! include_from_impls {
//     ($(#[$meta:meta])* $vis:vis struct $name:ident {
//         $($field_vis:vis $field_name:ident: $field_type:ty,)*
//     }) => {
//         $(#[$meta])*
//         $vis struct $name {
//             $($field_vis $field_name: $field_type,)*
//         }
//
//         impl From<$name> for fuel_core_types::fuel_tx::GasCosts {
//            fn from(value: $name) -> Self {
//                let values = fuel_core_types::fuel_tx::GasCostsValues {
//                    $($field_name: value.$field_name.into(),)*
//                };
//                Self::new(values)
//            }
//         }
//     }
// }

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

// TODO: Use macro
impl From<GasCosts> for fuel_core_types::fuel_tx::GasCosts {
    fn from(value: GasCosts) -> Self {
        let values = fuel_core_types::fuel_tx::GasCostsValues {
            add: value.add.into(),
            addi: value.addi.into(),
            aloc: value.aloc.into(),
            and: value.and.into(),
            andi: value.andi.into(),
            bal: value.bal.into(),
            bhei: value.bhei.into(),
            bhsh: value.bhsh.into(),
            burn: value.burn.into(),
            cb: value.cb.into(),
            cfei: value.cfei.into(),
            cfsi: value.cfsi.into(),
            croo: value.croo.into(),
            div: value.div.into(),
            divi: value.divi.into(),
            eck1: value.eck1.into(),
            ecr1: value.ecr1.into(),
            ed19: value.ed19.into(),
            eq: value.eq.into(),
            exp: value.exp.into(),
            expi: value.expi.into(),
            flag: value.flag.into(),
            gm: value.gm.into(),
            gt: value.gt.into(),
            gtf: value.gtf.into(),
            ji: value.ji.into(),
            jmp: value.jmp.into(),
            jne: value.jne.into(),
            jnei: value.jnei.into(),
            jnzi: value.jnzi.into(),
            jmpf: value.jmpf.into(),
            jmpb: value.jmpb.into(),
            jnzf: value.jnzf.into(),
            jnzb: value.jnzb.into(),
            jnef: value.jnef.into(),
            jneb: value.jneb.into(),
            k256: value.k256.into(),
            lb: value.lb.into(),
            log: value.log.into(),
            lt: value.lt.into(),
            lw: value.lw.into(),
            mcpi: value.mcpi.into(),
            mint: value.mint.into(),
            mlog: value.mlog.into(),
            mod_op: value.mod_op.into(),
            modi: value.modi.into(),
            move_op: value.move_op.into(),
            movi: value.movi.into(),
            mroo: value.mroo.into(),
            mul: value.mul.into(),
            muli: value.muli.into(),
            mldv: value.mldv.into(),
            noop: value.noop.into(),
            not: value.not.into(),
            or: value.or.into(),
            ori: value.ori.into(),
            ret: value.ret.into(),
            rvrt: value.rvrt.into(),
            s256: value.s256.into(),
            sb: value.sb.into(),
            scwq: value.scwq.into(),
            sll: value.sll.into(),
            slli: value.slli.into(),
            srl: value.srl.into(),
            srli: value.srli.into(),
            srw: value.srw.into(),
            sub: value.sub.into(),
            subi: value.subi.into(),
            sw: value.sw.into(),
            sww: value.sww.into(),
            swwq: value.swwq.into(),
            time: value.time.into(),
            tr: value.tr.into(),
            tro: value.tro.into(),
            wdcm: value.wdcm.into(),
            wqcm: value.wqcm.into(),
            wdop: value.wdop.into(),
            wqop: value.wqop.into(),
            wdml: value.wdml.into(),
            wqml: value.wqml.into(),
            wddv: value.wddv.into(),
            wqdv: value.wqdv.into(),
            wdmd: value.wdmd.into(),
            wqmd: value.wqmd.into(),
            wdam: value.wdam.into(),
            wqam: value.wqam.into(),
            wdmm: value.wdmm.into(),
            wqmm: value.wqmm.into(),
            xor: value.xor.into(),
            xori: value.xori.into(),

            call: value.call.into(),
            ccp: value.ccp.into(),
            csiz: value.csiz.into(),
            ldc: value.ldc.into(),
            logd: value.logd.into(),
            mcl: value.mcl.into(),
            mcli: value.mcli.into(),
            mcp: value.mcp.into(),
            meq: value.meq.into(),
            retd: value.retd.into(),
            smo: value.smo.into(),
            srwq: value.srwq.into(),
        };
        Self::new(values)
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct DependentCost {
    pub base: U64,
    pub dep_per_unit: U64,
}

impl From<DependentCost> for fuel_core_types::fuel_tx::DependentCost {
    fn from(value: DependentCost) -> Self {
        Self {
            base: value.base.into(),
            dep_per_unit: value.dep_per_unit.into(),
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
