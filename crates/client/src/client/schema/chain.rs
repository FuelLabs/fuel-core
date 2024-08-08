use crate::client::schema::{
    block::Block,
    schema,
    Address,
    AssetId,
    ConversionError,
    U16,
    U32,
    U64,
};

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ConsensusParameters {
    pub version: ConsensusParametersVersion,
    pub tx_params: TxParameters,
    pub predicate_params: PredicateParameters,
    pub script_params: ScriptParameters,
    pub contract_params: ContractParameters,
    pub fee_params: FeeParameters,
    pub base_asset_id: AssetId,
    pub block_gas_limit: U64,
    pub chain_id: U64,
    pub gas_costs: GasCosts,
    pub privileged_address: Address,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ConsensusParametersVersion {
    V1,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TxParameters {
    pub version: TxParametersVersion,
    pub max_inputs: U16,
    pub max_outputs: U16,
    pub max_witnesses: U32,
    pub max_gas_per_tx: U64,
    pub max_size: U64,
    pub max_bytecode_subsections: U16,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum TxParametersVersion {
    V1,
}

impl TryFrom<TxParameters> for fuel_core_types::fuel_tx::TxParameters {
    type Error = ConversionError;

    fn try_from(params: TxParameters) -> Result<Self, Self::Error> {
        match params.version {
            TxParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::TxParametersV1 {
                    max_inputs: params.max_inputs.into(),
                    max_outputs: params.max_outputs.into(),
                    max_witnesses: params.max_witnesses.into(),
                    max_gas_per_tx: params.max_gas_per_tx.into(),
                    max_size: params.max_size.into(),
                    max_bytecode_subsections: params.max_bytecode_subsections.into(),
                }
                .into(),
            ),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct PredicateParameters {
    pub version: PredicateParametersVersion,
    pub max_predicate_length: U64,
    pub max_predicate_data_length: U64,
    pub max_message_data_length: U64,
    pub max_gas_per_predicate: U64,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum PredicateParametersVersion {
    V1,
}

impl TryFrom<PredicateParameters> for fuel_core_types::fuel_tx::PredicateParameters {
    type Error = ConversionError;

    fn try_from(params: PredicateParameters) -> Result<Self, Self::Error> {
        match params.version {
            PredicateParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::PredicateParametersV1 {
                    max_predicate_length: params.max_predicate_length.into(),
                    max_predicate_data_length: params.max_predicate_data_length.into(),
                    max_message_data_length: params.max_message_data_length.into(),
                    max_gas_per_predicate: params.max_gas_per_predicate.into(),
                }
                .into(),
            ),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ScriptParameters {
    pub version: ScriptParametersVersion,
    pub max_script_length: U64,
    pub max_script_data_length: U64,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ScriptParametersVersion {
    V1,
}

impl TryFrom<ScriptParameters> for fuel_core_types::fuel_tx::ScriptParameters {
    type Error = ConversionError;

    fn try_from(params: ScriptParameters) -> Result<Self, Self::Error> {
        match params.version {
            ScriptParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::ScriptParametersV1 {
                    max_script_length: params.max_script_length.into(),
                    max_script_data_length: params.max_script_data_length.into(),
                }
                .into(),
            ),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractParameters {
    pub version: ContractParametersVersion,
    pub contract_max_size: U64,
    pub max_storage_slots: U64,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ContractParametersVersion {
    V1,
}

impl TryFrom<ContractParameters> for fuel_core_types::fuel_tx::ContractParameters {
    type Error = ConversionError;

    fn try_from(params: ContractParameters) -> Result<Self, Self::Error> {
        match params.version {
            ContractParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::ContractParametersV1 {
                    contract_max_size: params.contract_max_size.into(),
                    max_storage_slots: params.max_storage_slots.into(),
                }
                .into(),
            ),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct FeeParameters {
    pub version: FeeParametersVersion,
    pub gas_price_factor: U64,
    pub gas_per_byte: U64,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum FeeParametersVersion {
    V1,
}

impl TryFrom<FeeParameters> for fuel_core_types::fuel_tx::FeeParameters {
    type Error = ConversionError;

    fn try_from(params: FeeParameters) -> Result<Self, Self::Error> {
        match params.version {
            FeeParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::FeeParametersV1 {
                    gas_price_factor: params.gas_price_factor.into(),
                    gas_per_byte: params.gas_per_byte.into(),
                }
                .into(),
            ),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct GasCosts {
    pub version: GasCostsVersion,
    pub add: U64,
    pub addi: U64,
    pub and: U64,
    pub andi: U64,
    pub bal: U64,
    pub bhei: U64,
    pub bhsh: U64,
    pub burn: U64,
    pub cb: U64,
    pub cfsi: U64,
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

    pub aloc_dependent_cost: DependentCost,
    pub bsiz: Option<DependentCost>,
    pub bldd: Option<DependentCost>,
    pub cfe: DependentCost,
    pub cfei_dependent_cost: DependentCost,
    pub call: DependentCost,
    pub ccp: DependentCost,
    pub croo: DependentCost,
    pub csiz: DependentCost,
    pub ed19_dependent_cost: DependentCost,
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

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum GasCostsVersion {
    V1,
}

impl TryFrom<GasCosts> for fuel_core_types::fuel_tx::GasCosts {
    type Error = ConversionError;

    fn try_from(value: GasCosts) -> Result<Self, Self::Error> {
        match value.version {
            GasCostsVersion::V1 => Ok(fuel_core_types::fuel_tx::GasCosts::new(
                fuel_core_types::fuel_tx::consensus_parameters::gas::GasCostsValuesV4 {
                    add: value.add.into(),
                    addi: value.addi.into(),
                    and: value.and.into(),
                    andi: value.andi.into(),
                    bal: value.bal.into(),
                    bhei: value.bhei.into(),
                    bhsh: value.bhsh.into(),
                    burn: value.burn.into(),
                    cb: value.cb.into(),
                    cfsi: value.cfsi.into(),
                    div: value.div.into(),
                    divi: value.divi.into(),
                    eck1: value.eck1.into(),
                    ecr1: value.ecr1.into(),
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
                    lb: value.lb.into(),
                    log: value.log.into(),
                    lt: value.lt.into(),
                    lw: value.lw.into(),
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
                    poph: value.poph.into(),
                    popl: value.popl.into(),
                    pshh: value.pshh.into(),
                    pshl: value.pshl.into(),
                    ret: value.ret.into(),
                    rvrt: value.rvrt.into(),
                    sb: value.sb.into(),
                    sll: value.sll.into(),
                    slli: value.slli.into(),
                    srl: value.srl.into(),
                    srli: value.srli.into(),
                    srw: value.srw.into(),
                    sub: value.sub.into(),
                    subi: value.subi.into(),
                    sw: value.sw.into(),
                    sww: value.sww.into(),
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

                    aloc: value.aloc_dependent_cost.into(),
                    bsiz: value.bsiz.map(Into::into).unwrap_or(fuel_core_types::fuel_tx::consensus_parameters::DependentCost::free()),
                    bldd: value.bldd.map(Into::into).unwrap_or(fuel_core_types::fuel_tx::consensus_parameters::DependentCost::free()),
                    cfe: value.cfe.into(),
                    cfei: value.cfei_dependent_cost.into(),
                    call: value.call.into(),
                    ccp: value.ccp.into(),
                    croo: value.croo.into(),
                    csiz: value.csiz.into(),
                    ed19: value.ed19_dependent_cost.into(),
                    k256: value.k256.into(),
                    ldc: value.ldc.into(),
                    logd: value.logd.into(),
                    mcl: value.mcl.into(),
                    mcli: value.mcli.into(),
                    mcp: value.mcp.into(),
                    mcpi: value.mcpi.into(),
                    meq: value.meq.into(),
                    retd: value.retd.into(),
                    s256: value.s256.into(),
                    scwq: value.scwq.into(),
                    smo: value.smo.into(),
                    srwq: value.srwq.into(),
                    swwq: value.swwq.into(),
                    contract_root: value.contract_root.into(),
                    state_root: value.state_root.into(),
                    vm_initialization: value.vm_initialization.into(),
                    new_storage_per_byte: value.new_storage_per_byte.into(),
                }
                .into(),
            )),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct LightOperation {
    pub base: U64,
    pub units_per_gas: U64,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct HeavyOperation {
    pub base: U64,
    pub gas_per_unit: U64,
}

#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum DependentCost {
    LightOperation(LightOperation),
    HeavyOperation(HeavyOperation),
    #[cynic(fallback)]
    Unknown,
}

impl TryFrom<ConsensusParameters> for fuel_core_types::fuel_tx::ConsensusParameters {
    type Error = ConversionError;

    fn try_from(params: ConsensusParameters) -> Result<Self, Self::Error> {
        match params.version {
            ConsensusParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::ConsensusParametersV1 {
                    tx_params: params.tx_params.try_into()?,
                    predicate_params: params.predicate_params.try_into()?,
                    script_params: params.script_params.try_into()?,
                    contract_params: params.contract_params.try_into()?,
                    fee_params: params.fee_params.try_into()?,
                    base_asset_id: params.base_asset_id.into(),
                    block_gas_limit: params.block_gas_limit.into(),
                    chain_id: params.chain_id.0.into(),
                    gas_costs: params.gas_costs.try_into()?,
                    privileged_address: params.privileged_address.into(),
                }
                .into(),
            ),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct ChainQuery {
    pub chain: ChainInfo,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
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
