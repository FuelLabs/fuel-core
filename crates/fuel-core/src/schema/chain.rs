use crate::{
    fuel_core_graphql_api::{
        api_service::ChainInfoProvider,
        query_costs,
    },
    graphql_api::Config,
    schema::{
        ReadViewProvider,
        block::Block,
        scalars::{
            Address,
            AssetId,
            U16,
            U32,
            U64,
        },
    },
};
use async_graphql::{
    Context,
    Enum,
    Lookahead,
    Object,
    Union,
};
use fuel_core_types::{
    fuel_tx,
    fuel_tx::{
        GasCostsValues,
        consensus_parameters::gas::GasCostsValuesV6,
    },
};
use std::{
    ops::Deref,
    sync::Arc,
};

pub struct ChainInfo;
pub struct ConsensusParameters(pub Arc<fuel_tx::ConsensusParameters>);
pub struct TxParameters(fuel_tx::TxParameters);
pub struct PredicateParameters(fuel_tx::PredicateParameters);
pub struct ScriptParameters(fuel_tx::ScriptParameters);
pub struct ContractParameters(fuel_tx::ContractParameters);
pub struct FeeParameters(fuel_tx::FeeParameters);

pub struct GasCosts(fuel_tx::GasCosts);

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum GasCostsVersion {
    V1,
}

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum FeeParametersVersion {
    V1,
}

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum ContractParametersVersion {
    V1,
}

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum ScriptParametersVersion {
    V1,
    V2,
}

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum PredicateParametersVersion {
    V1,
}

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum TxParametersVersion {
    V1,
}

#[derive(Clone, Copy, Debug, Enum, Eq, PartialEq)]
pub enum ConsensusParametersVersion {
    V1,
}

#[derive(Union)]
pub enum DependentCost {
    LightOperation(LightOperation),
    HeavyOperation(HeavyOperation),
}

pub struct LightOperation {
    base: u64,
    units_per_gas: u64,
}

pub struct HeavyOperation {
    base: u64,
    gas_per_unit: u64,
}

fn requires_v2_consensus_parameters(look_ahead: &Lookahead<'_>) -> bool {
    let script_params_require_v2 = look_ahead
        .field("scriptParams")
        .field("maxStorageSlotLength")
        .exists();
    let consensus_params_require_v2 =
        look_ahead.field("blockTransactionSizeLimit").exists();
    let gas_costs_require_v2 = [
        "storageReadCold",
        "storageReadHot",
        "storageWrite",
        "storageClear",
    ]
    .into_iter()
    .any(|field| look_ahead.field("gasCosts").field(field).exists());

    consensus_params_require_v2 || script_params_require_v2 || gas_costs_require_v2
}

#[allow(unused)]
fn script_params_as_v1(
    script_params: fuel_tx::ScriptParameters,
) -> fuel_tx::ScriptParameters {
    match script_params {
        fuel_tx::ScriptParameters::V1(params) => params.into(),
        fuel_tx::ScriptParameters::V2(params) => {
            fuel_tx::consensus_parameters::ScriptParametersV1 {
                max_script_length: params.max_script_length,
                max_script_data_length: params.max_script_data_length,
            }
            .into()
        }
    }
}

#[allow(unused)]
fn gas_costs_as_v6(gas_costs: fuel_tx::GasCosts) -> fuel_tx::GasCosts {
    use fuel_tx::consensus_parameters::DependentCost;

    match gas_costs.deref() {
        GasCostsValues::V1(_)
        | GasCostsValues::V2(_)
        | GasCostsValues::V3(_)
        | GasCostsValues::V4(_)
        | GasCostsValues::V5(_)
        | GasCostsValues::V6(_) => gas_costs,
        GasCostsValues::V7(values) => fuel_tx::GasCosts::new(
            GasCostsValuesV6 {
                add: values.add,
                addi: values.addi,
                and: values.and,
                andi: values.andi,
                bal: values.bal,
                bhei: values.bhei,
                bhsh: values.bhsh,
                burn: values.burn,
                cb: values.cb,
                cfsi: values.cfsi,
                div: values.div,
                divi: values.divi,
                eck1: values.eck1,
                ecr1: values.ecr1,
                eq: values.eq,
                exp: values.exp,
                expi: values.expi,
                flag: values.flag,
                gm: values.gm,
                gt: values.gt,
                gtf: values.gtf,
                ji: values.ji,
                jmp: values.jmp,
                jne: values.jne,
                jnei: values.jnei,
                jnzi: values.jnzi,
                jmpf: values.jmpf,
                jmpb: values.jmpb,
                jnzf: values.jnzf,
                jnzb: values.jnzb,
                jnef: values.jnef,
                jneb: values.jneb,
                lb: values.lb,
                log: values.log,
                lt: values.lt,
                lw: values.lw,
                mint: values.mint,
                mlog: values.mlog,
                mod_op: values.mod_op,
                modi: values.modi,
                move_op: values.move_op,
                movi: values.movi,
                mroo: values.mroo,
                mul: values.mul,
                muli: values.muli,
                mldv: values.mldv,
                niop: values.niop,
                noop: values.noop,
                not: values.not,
                or: values.or,
                ori: values.ori,
                poph: values.poph,
                popl: values.popl,
                pshh: values.pshh,
                pshl: values.pshl,
                ret: values.ret,
                rvrt: values.rvrt,
                sb: values.sb,
                sll: values.sll,
                slli: values.slli,
                srl: values.srl,
                srli: values.srli,
                srw: 0,
                sub: values.sub,
                subi: values.subi,
                sw: values.sw,
                sww: 0,
                time: values.time,
                tr: values.tr,
                tro: values.tro,
                wdcm: values.wdcm,
                wqcm: values.wqcm,
                wdop: values.wdop,
                wqop: values.wqop,
                wdml: values.wdml,
                wqml: values.wqml,
                wddv: values.wddv,
                wqdv: values.wqdv,
                wdmd: values.wdmd,
                wqmd: values.wqmd,
                wdam: values.wdam,
                wqam: values.wqam,
                wdmm: values.wdmm,
                wqmm: values.wqmm,
                xor: values.xor,
                xori: values.xori,
                ecop: values.ecop,
                aloc: values.aloc,
                bsiz: values.bsiz,
                bldd: values.bldd,
                cfe: values.cfe,
                cfei: values.cfei,
                call: values.call,
                ccp: values.ccp,
                croo: values.croo,
                csiz: values.csiz,
                ed19: values.ed19,
                k256: values.k256,
                ldc: values.ldc,
                logd: values.logd,
                mcl: values.mcl,
                mcli: values.mcli,
                mcp: values.mcp,
                mcpi: values.mcpi,
                meq: values.meq,
                retd: values.retd,
                s256: values.s256,
                scwq: DependentCost::free(),
                smo: values.smo,
                srwq: DependentCost::free(),
                swwq: DependentCost::free(),
                epar: values.epar,
                contract_root: values.contract_root,
                state_root: values.state_root,
                vm_initialization: values.vm_initialization,
                new_storage_per_byte: values.new_storage_per_byte,
            }
            .into(),
        ),
    }
}

pub(crate) fn consensus_params_for_selection(
    params: Arc<fuel_tx::ConsensusParameters>,
    look_ahead: &Lookahead<'_>,
) -> Arc<fuel_tx::ConsensusParameters> {
    if requires_v2_consensus_parameters(look_ahead) {
        return params;
    }

    match params.as_ref() {
        fuel_tx::ConsensusParameters::V1(_) => params,
        fuel_tx::ConsensusParameters::V2(params) => Arc::new(
            fuel_tx::consensus_parameters::ConsensusParametersV1 {
                tx_params: params.tx_params,
                predicate_params: params.predicate_params,
                script_params: script_params_as_v1(params.script_params),
                contract_params: params.contract_params,
                fee_params: params.fee_params,
                chain_id: params.chain_id,
                gas_costs: gas_costs_as_v6(params.gas_costs.clone()),
                base_asset_id: params.base_asset_id,
                block_gas_limit: params.block_gas_limit,
                privileged_address: params.privileged_address,
            }
            .into(),
        ),
    }
}

impl From<fuel_tx::DependentCost> for DependentCost {
    fn from(value: fuel_tx::DependentCost) -> Self {
        match value {
            fuel_tx::DependentCost::LightOperation {
                base,
                units_per_gas,
            } => DependentCost::LightOperation(LightOperation {
                base,
                units_per_gas,
            }),
            fuel_tx::DependentCost::HeavyOperation { base, gas_per_unit } => {
                DependentCost::HeavyOperation(HeavyOperation { base, gas_per_unit })
            }
        }
    }
}

#[Object]
impl ConsensusParameters {
    async fn version(&self) -> ConsensusParametersVersion {
        match self.0.as_ref() {
            fuel_tx::ConsensusParameters::V1(_) | fuel_tx::ConsensusParameters::V2(_) => {
                ConsensusParametersVersion::V1
            }
        }
    }

    async fn tx_params(&self) -> TxParameters {
        TxParameters(self.0.tx_params().to_owned())
    }

    async fn predicate_params(&self) -> PredicateParameters {
        PredicateParameters(self.0.predicate_params().to_owned())
    }

    async fn script_params(&self) -> ScriptParameters {
        ScriptParameters(self.0.script_params().to_owned())
    }

    async fn contract_params(&self) -> ContractParameters {
        ContractParameters(self.0.contract_params().to_owned())
    }

    async fn fee_params(&self) -> FeeParameters {
        FeeParameters(self.0.fee_params().to_owned())
    }

    async fn base_asset_id(&self) -> AssetId {
        AssetId(*self.0.base_asset_id())
    }

    async fn block_gas_limit(&self) -> U64 {
        self.0.block_gas_limit().into()
    }

    async fn block_transaction_size_limit(&self) -> U64 {
        self.0.block_transaction_size_limit().into()
    }

    async fn chain_id(&self) -> U64 {
        (*self.0.chain_id()).into()
    }

    async fn gas_costs(&self) -> async_graphql::Result<GasCosts> {
        Ok(GasCosts(self.0.gas_costs().clone()))
    }

    async fn privileged_address(&self) -> async_graphql::Result<Address> {
        Ok(Address(*self.0.privileged_address()))
    }
}

#[Object]
impl TxParameters {
    async fn version(&self) -> TxParametersVersion {
        match self.0 {
            fuel_tx::TxParameters::V1(_) => TxParametersVersion::V1,
        }
    }

    async fn max_inputs(&self) -> U16 {
        self.0.max_inputs().into()
    }

    async fn max_outputs(&self) -> U16 {
        self.0.max_outputs().into()
    }

    async fn max_witnesses(&self) -> U32 {
        self.0.max_witnesses().into()
    }

    async fn max_gas_per_tx(&self) -> U64 {
        self.0.max_gas_per_tx().into()
    }

    async fn max_size(&self) -> U64 {
        self.0.max_size().into()
    }

    async fn max_bytecode_subsections(&self) -> U16 {
        self.0.max_bytecode_subsections().into()
    }
}

#[Object]
impl PredicateParameters {
    async fn version(&self) -> PredicateParametersVersion {
        match self.0 {
            fuel_tx::PredicateParameters::V1(_) => PredicateParametersVersion::V1,
        }
    }

    async fn max_predicate_length(&self) -> U64 {
        self.0.max_predicate_length().into()
    }

    async fn max_predicate_data_length(&self) -> U64 {
        self.0.max_predicate_data_length().into()
    }

    async fn max_gas_per_predicate(&self) -> U64 {
        self.0.max_gas_per_predicate().into()
    }

    async fn max_message_data_length(&self) -> U64 {
        self.0.max_message_data_length().into()
    }
}

#[Object]
impl ScriptParameters {
    async fn version(&self) -> ScriptParametersVersion {
        match self.0 {
            fuel_tx::ScriptParameters::V1(_) => ScriptParametersVersion::V1,
            fuel_tx::ScriptParameters::V2(_) => ScriptParametersVersion::V2,
        }
    }

    async fn max_script_length(&self) -> U64 {
        self.0.max_script_length().into()
    }

    async fn max_script_data_length(&self) -> U64 {
        self.0.max_script_data_length().into()
    }

    async fn max_storage_slot_length(&self) -> U64 {
        self.0.max_storage_slot_length().into()
    }
}

#[Object]
impl ContractParameters {
    async fn version(&self) -> ContractParametersVersion {
        match self.0 {
            fuel_tx::ContractParameters::V1(_) => ContractParametersVersion::V1,
        }
    }

    async fn contract_max_size(&self) -> U64 {
        self.0.contract_max_size().into()
    }

    async fn max_storage_slots(&self) -> U64 {
        self.0.max_storage_slots().into()
    }
}

#[Object]
impl FeeParameters {
    async fn version(&self) -> FeeParametersVersion {
        match self.0 {
            fuel_tx::FeeParameters::V1(_) => FeeParametersVersion::V1,
        }
    }

    async fn gas_price_factor(&self) -> U64 {
        self.0.gas_price_factor().into()
    }

    async fn gas_per_byte(&self) -> U64 {
        self.0.gas_per_byte().into()
    }
}

#[Object]
impl GasCosts {
    async fn version(&self) -> GasCostsVersion {
        match self.0.deref() {
            GasCostsValues::V1(_)
            | GasCostsValues::V2(_)
            | GasCostsValues::V3(_)
            | GasCostsValues::V4(_)
            | GasCostsValues::V5(_)
            | GasCostsValues::V6(_)
            | GasCostsValues::V7(_) => GasCostsVersion::V1,
        }
    }

    async fn add(&self) -> U64 {
        self.0.add().into()
    }

    async fn addi(&self) -> U64 {
        self.0.addi().into()
    }

    async fn aloc(&self) -> U64 {
        self.0.aloc().base().into()
    }

    async fn and(&self) -> U64 {
        self.0.and().into()
    }

    async fn andi(&self) -> U64 {
        self.0.andi().into()
    }

    async fn bal(&self) -> U64 {
        self.0.bal().into()
    }

    async fn bhei(&self) -> U64 {
        self.0.bhei().into()
    }

    async fn bhsh(&self) -> U64 {
        self.0.bhsh().into()
    }

    async fn burn(&self) -> U64 {
        self.0.burn().into()
    }

    async fn cb(&self) -> U64 {
        self.0.cb().into()
    }

    async fn cfei(&self) -> U64 {
        self.0.cfei().base().into()
    }

    async fn cfsi(&self) -> U64 {
        self.0.cfsi().into()
    }

    async fn div(&self) -> U64 {
        self.0.div().into()
    }

    async fn divi(&self) -> U64 {
        self.0.divi().into()
    }

    async fn ecr1(&self) -> U64 {
        self.0.ecr1().into()
    }

    async fn eck1(&self) -> U64 {
        self.0.eck1().into()
    }

    async fn ed19(&self) -> U64 {
        self.0.ed19().base().into()
    }

    async fn eq(&self) -> U64 {
        self.0.eq_().into()
    }

    async fn exp(&self) -> U64 {
        self.0.exp().into()
    }

    async fn expi(&self) -> U64 {
        self.0.expi().into()
    }

    async fn flag(&self) -> U64 {
        self.0.flag().into()
    }

    async fn gm(&self) -> U64 {
        self.0.gm().into()
    }

    async fn gt(&self) -> U64 {
        self.0.gt().into()
    }

    async fn gtf(&self) -> U64 {
        self.0.gtf().into()
    }

    async fn ji(&self) -> U64 {
        self.0.ji().into()
    }

    async fn jmp(&self) -> U64 {
        self.0.jmp().into()
    }

    async fn jne(&self) -> U64 {
        self.0.jne().into()
    }

    async fn jnei(&self) -> U64 {
        self.0.jnei().into()
    }

    async fn jnzi(&self) -> U64 {
        self.0.jnzi().into()
    }

    async fn jmpf(&self) -> U64 {
        self.0.jmpf().into()
    }

    async fn jmpb(&self) -> U64 {
        self.0.jmpb().into()
    }

    async fn jnzf(&self) -> U64 {
        self.0.jnzf().into()
    }

    async fn jnzb(&self) -> U64 {
        self.0.jnzb().into()
    }

    async fn jnef(&self) -> U64 {
        self.0.jnef().into()
    }

    async fn jneb(&self) -> U64 {
        self.0.jneb().into()
    }

    async fn lb(&self) -> U64 {
        self.0.lb().into()
    }

    async fn log(&self) -> U64 {
        self.0.log().into()
    }

    async fn lt(&self) -> U64 {
        self.0.lt().into()
    }

    async fn lw(&self) -> U64 {
        self.0.lw().into()
    }

    async fn mint(&self) -> U64 {
        self.0.mint().into()
    }

    async fn mlog(&self) -> U64 {
        self.0.mlog().into()
    }

    async fn mod_op(&self) -> U64 {
        self.0.mod_op().into()
    }

    async fn modi(&self) -> U64 {
        self.0.modi().into()
    }

    async fn move_op(&self) -> U64 {
        self.0.move_op().into()
    }

    async fn movi(&self) -> U64 {
        self.0.movi().into()
    }

    async fn mroo(&self) -> U64 {
        self.0.mroo().into()
    }

    async fn mul(&self) -> U64 {
        self.0.mul().into()
    }

    async fn muli(&self) -> U64 {
        self.0.muli().into()
    }

    async fn mldv(&self) -> U64 {
        self.0.mldv().into()
    }

    async fn niop(&self) -> Option<U64> {
        self.0.niop().ok().map(Into::into)
    }

    async fn noop(&self) -> U64 {
        self.0.noop().into()
    }

    async fn not(&self) -> U64 {
        self.0.not().into()
    }

    async fn or(&self) -> U64 {
        self.0.or().into()
    }

    async fn ori(&self) -> U64 {
        self.0.ori().into()
    }

    async fn poph(&self) -> U64 {
        self.0.poph().into()
    }

    async fn popl(&self) -> U64 {
        self.0.popl().into()
    }

    async fn pshh(&self) -> U64 {
        self.0.pshh().into()
    }

    async fn pshl(&self) -> U64 {
        self.0.pshl().into()
    }

    async fn ret(&self) -> U64 {
        self.0.ret().into()
    }

    async fn rvrt(&self) -> U64 {
        self.0.rvrt().into()
    }

    async fn sb(&self) -> U64 {
        self.0.sb().into()
    }

    async fn sll(&self) -> U64 {
        self.0.sll().into()
    }

    async fn slli(&self) -> U64 {
        self.0.slli().into()
    }

    async fn srl(&self) -> U64 {
        self.0.srl().into()
    }

    async fn srli(&self) -> U64 {
        self.0.srli().into()
    }

    async fn srw(&self) -> Option<U64> {
        self.0.srw().ok().map(Into::into)
    }

    async fn sub(&self) -> U64 {
        self.0.sub().into()
    }

    async fn subi(&self) -> U64 {
        self.0.subi().into()
    }

    async fn sw(&self) -> U64 {
        self.0.sw().into()
    }

    async fn sww(&self) -> Option<U64> {
        self.0.sww().ok().map(Into::into)
    }

    async fn time(&self) -> U64 {
        self.0.time().into()
    }

    async fn tr(&self) -> U64 {
        self.0.tr().into()
    }

    async fn tro(&self) -> U64 {
        self.0.tro().into()
    }

    async fn wdcm(&self) -> U64 {
        self.0.wdcm().into()
    }

    async fn wqcm(&self) -> U64 {
        self.0.wqcm().into()
    }

    async fn wdop(&self) -> U64 {
        self.0.wdop().into()
    }

    async fn wqop(&self) -> U64 {
        self.0.wqop().into()
    }

    async fn wdml(&self) -> U64 {
        self.0.wdml().into()
    }

    async fn wqml(&self) -> U64 {
        self.0.wqml().into()
    }

    async fn wddv(&self) -> U64 {
        self.0.wddv().into()
    }

    async fn wqdv(&self) -> U64 {
        self.0.wqdv().into()
    }

    async fn wdmd(&self) -> U64 {
        self.0.wdmd().into()
    }

    async fn wqmd(&self) -> U64 {
        self.0.wqmd().into()
    }

    async fn wdam(&self) -> U64 {
        self.0.wdam().into()
    }

    async fn wqam(&self) -> U64 {
        self.0.wqam().into()
    }

    async fn wdmm(&self) -> U64 {
        self.0.wdmm().into()
    }

    async fn wqmm(&self) -> U64 {
        self.0.wqmm().into()
    }

    async fn xor(&self) -> U64 {
        self.0.xor().into()
    }

    async fn xori(&self) -> U64 {
        self.0.xori().into()
    }

    async fn ecop(&self) -> Option<U64> {
        self.0.ecop().ok().map(Into::into)
    }

    async fn aloc_dependent_cost(&self) -> DependentCost {
        self.0.aloc().into()
    }

    async fn bldd(&self) -> Option<DependentCost> {
        self.0.bldd().ok().map(Into::into)
    }

    async fn bsiz(&self) -> Option<DependentCost> {
        self.0.bsiz().ok().map(Into::into)
    }

    async fn cfe(&self) -> DependentCost {
        self.0.cfe().into()
    }

    async fn cfei_dependent_cost(&self) -> DependentCost {
        self.0.cfei().into()
    }

    async fn call(&self) -> DependentCost {
        self.0.call().into()
    }

    async fn ccp(&self) -> DependentCost {
        self.0.ccp().into()
    }

    async fn croo(&self) -> DependentCost {
        self.0.croo().into()
    }

    async fn csiz(&self) -> DependentCost {
        self.0.csiz().into()
    }

    async fn ed19_dependent_cost(&self) -> DependentCost {
        self.0.ed19().into()
    }

    async fn k256(&self) -> DependentCost {
        self.0.k256().into()
    }

    async fn ldc(&self) -> DependentCost {
        self.0.ldc().into()
    }

    async fn logd(&self) -> DependentCost {
        self.0.logd().into()
    }

    async fn mcl(&self) -> DependentCost {
        self.0.mcl().into()
    }

    async fn mcli(&self) -> DependentCost {
        self.0.mcli().into()
    }

    async fn mcp(&self) -> DependentCost {
        self.0.mcp().into()
    }

    async fn mcpi(&self) -> DependentCost {
        self.0.mcpi().into()
    }

    async fn meq(&self) -> DependentCost {
        self.0.meq().into()
    }

    async fn retd(&self) -> DependentCost {
        self.0.retd().into()
    }

    async fn s256(&self) -> DependentCost {
        self.0.s256().into()
    }

    async fn scwq(&self) -> Option<DependentCost> {
        self.0.scwq().ok().map(Into::into)
    }

    async fn smo(&self) -> DependentCost {
        self.0.smo().into()
    }

    async fn srwq(&self) -> Option<DependentCost> {
        self.0.srwq().ok().map(Into::into)
    }

    async fn swwq(&self) -> Option<DependentCost> {
        self.0.swwq().ok().map(Into::into)
    }

    async fn epar(&self) -> Option<DependentCost> {
        self.0.epar().ok().map(Into::into)
    }

    // Storage micro-ops

    async fn storage_read_cold(&self) -> Option<DependentCost> {
        self.0.storage_read_cold().ok().map(Into::into)
    }

    async fn storage_read_hot(&self) -> Option<DependentCost> {
        self.0.storage_read_hot().ok().map(Into::into)
    }

    async fn storage_write(&self) -> Option<DependentCost> {
        self.0.storage_write().ok().map(Into::into)
    }

    async fn storage_clear(&self) -> Option<DependentCost> {
        self.0.storage_clear().ok().map(Into::into)
    }

    // Non-opcode prices

    async fn contract_root(&self) -> DependentCost {
        self.0.contract_root().into()
    }

    async fn state_root(&self) -> DependentCost {
        self.0.state_root().into()
    }

    async fn vm_initialization(&self) -> DependentCost {
        self.0.vm_initialization().into()
    }

    async fn new_storage_per_byte(&self) -> U64 {
        self.0.new_storage_per_byte().into()
    }
}

#[Object]
impl LightOperation {
    async fn base(&self) -> U64 {
        self.base.into()
    }

    async fn units_per_gas(&self) -> U64 {
        self.units_per_gas.into()
    }
}

#[Object]
impl HeavyOperation {
    async fn base(&self) -> U64 {
        self.base.into()
    }

    async fn gas_per_unit(&self) -> U64 {
        self.gas_per_unit.into()
    }
}

#[Object]
impl ChainInfo {
    #[graphql(complexity = "query_costs().storage_read")]
    async fn name(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let config: &Config = ctx.data_unchecked();
        Ok(config.chain_name.clone())
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn latest_block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query = ctx.read_view()?;

        let latest_block = query.latest_block()?.into();
        Ok(latest_block)
    }

    #[graphql(complexity = "query_costs().storage_read")]
    async fn da_height(&self, ctx: &Context<'_>) -> U64 {
        let Ok(query) = ctx.read_view() else {
            return 0.into();
        };

        query.da_height().unwrap_or_default().0.into()
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn consensus_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ConsensusParameters> {
        let params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();

        Ok(ConsensusParameters(consensus_params_for_selection(
            params,
            &ctx.look_ahead(),
        )))
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn gas_costs(&self, ctx: &Context<'_>) -> async_graphql::Result<GasCosts> {
        let params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();

        Ok(GasCosts(params.gas_costs().clone()))
    }
}

#[derive(Default)]
pub struct ChainQuery;

#[Object]
impl ChainQuery {
    async fn chain(&self) -> ChainInfo {
        ChainInfo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        graphql_api::{
            Config,
            ServiceConfig,
            ports::MockChainStateProvider,
        },
        schema::build_schema,
    };
    use async_graphql::Request;
    use fuel_tx::consensus_parameters::ConsensusParametersV2;
    use std::{
        net::{
            IpAddr,
            Ipv4Addr,
            SocketAddr,
        },
        time::Duration,
    };

    #[test]
    fn gas_costs_downgrade_to_v6_fills_legacy_only_fields() {
        use fuel_tx::consensus_parameters::DependentCost;

        let downgraded =
            gas_costs_as_v6(fuel_tx::ConsensusParameters::standard().gas_costs().clone());

        let GasCostsValues::V6(values) = downgraded.deref() else {
            panic!("expected downgraded gas costs to be V6");
        };

        assert_eq!(values.srw, 0);
        assert_eq!(values.sww, 0);
        assert_eq!(values.scwq, DependentCost::free());
        assert_eq!(values.srwq, DependentCost::free());
        assert_eq!(values.swwq, DependentCost::free());
    }

    #[tokio::test]
    async fn v047_shape_query_keeps_block_transaction_size_limit() {
        const BLOCK_TRANSACTION_SIZE_LIMIT: u64 = 1_234_567;

        let mut params = ConsensusParametersV2::standard();
        params.block_transaction_size_limit = BLOCK_TRANSACTION_SIZE_LIMIT;

        let mut mocked_provider = MockChainStateProvider::default();
        mocked_provider
            .expect_current_consensus_params()
            .return_const(Arc::new(params.into()));

        let config = Config {
            config: ServiceConfig {
                addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                number_of_threads: 1,
                database_batch_size: 1,
                block_subscriptions_queue: 1,
                max_queries_depth: 16,
                max_queries_complexity: 10_000,
                max_queries_recursive_depth: 32,
                max_queries_resolver_recursive_depth: 32,
                max_queries_directives: 16,
                max_concurrent_queries: 16,
                request_body_bytes_limit: 1024 * 1024,
                required_fuel_block_height_tolerance: 0,
                required_fuel_block_height_timeout: Duration::from_secs(1),
                query_log_threshold_time: Duration::from_secs(1),
                api_request_timeout: Duration::from_secs(1),
                assemble_tx_dry_run_limit: 1,
                assemble_tx_estimate_predicates_limit: 1,
                costs: Default::default(),
            },
            utxo_validation: false,
            debug: false,
            allow_syscall: false,
            historical_execution: false,
            expensive_subscriptions: false,
            max_tx: 1,
            max_gas: 1,
            max_size: 1,
            max_txpool_dependency_chain_length: 1,
            chain_name: "test".into(),
        };

        let schema = build_schema()
            .data(config)
            .data(Box::new(mocked_provider)
                as crate::graphql_api::api_service::ChainInfoProvider)
            .finish();

        let response = schema
            .execute(Request::new(
                "{ chain { consensusParameters { blockTransactionSizeLimit } } }",
            ))
            .await;

        assert!(
            response.errors.is_empty(),
            "unexpected GraphQL errors: {:?}",
            response.errors
        );
        assert_eq!(
            response.data.into_json().unwrap(),
            serde_json::json!({
                "chain": {
                    "consensusParameters": {
                        "blockTransactionSizeLimit": BLOCK_TRANSACTION_SIZE_LIMIT.to_string()
                    }
                }
            })
        );
    }
}
