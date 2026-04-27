use crate::client::schema::{
    Address,
    AssetId,
    ConversionError,
    U16,
    U32,
    U64,
    block::Block,
    schema,
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
    pub block_transaction_size_limit: U64,
    pub chain_id: U64,
    pub gas_costs: GasCosts,
    pub privileged_address: Address,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ConsensusParametersVersion {
    V1,
    #[cynic(fallback)]
    Unknown,
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
    #[cynic(fallback)]
    Unknown,
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
            _ => Err(ConversionError::UnknownVariant("TxParametersVersion")),
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
    #[cynic(fallback)]
    Unknown,
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
            _ => Err(ConversionError::UnknownVariant(
                "PredicateParametersVersion",
            )),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ScriptParameters {
    pub version: ScriptParametersVersion,
    pub max_script_length: U64,
    pub max_script_data_length: U64,
    pub max_storage_slot_length: U64,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ScriptParametersVersion {
    V1,
    V2,
    #[cynic(fallback)]
    Unknown,
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
            ScriptParametersVersion::V2 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::ScriptParametersV2 {
                    max_script_length: params.max_script_length.into(),
                    max_script_data_length: params.max_script_data_length.into(),
                    max_storage_slot_length: params.max_storage_slot_length.into(),
                }
                .into(),
            ),
            _ => Err(ConversionError::UnknownVariant("ScriptParametersVersion")),
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
    #[cynic(fallback)]
    Unknown,
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
            _ => Err(ConversionError::UnknownVariant("ContractParametersVersion")),
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
    #[cynic(fallback)]
    Unknown,
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
            _ => Err(ConversionError::UnknownVariant("FeeParametersVersion")),
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
    pub niop: Option<U64>,
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
    pub srw: Option<U64>,
    pub sub: U64,
    pub subi: U64,
    pub sw: U64,
    pub sww: Option<U64>,
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
    pub ecop: Option<U64>,

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
    pub scwq: Option<DependentCost>,
    pub smo: DependentCost,
    pub srwq: Option<DependentCost>,
    pub swwq: Option<DependentCost>,
    pub epar: Option<DependentCost>,

    // V7 storage operation costs (absent in V6)
    pub storage_read_cold: Option<DependentCost>,
    pub storage_read_hot: Option<DependentCost>,
    pub storage_write: Option<DependentCost>,
    pub storage_clear: Option<DependentCost>,

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
    #[cynic(fallback)]
    Unknown,
}

impl TryFrom<GasCosts> for fuel_core_types::fuel_tx::GasCosts {
    type Error = ConversionError;

    fn try_from(value: GasCosts) -> Result<Self, Self::Error> {
        use fuel_core_types::fuel_tx::consensus_parameters::{
            DependentCost,
            gas::{
                GasCostsValuesV6,
                GasCostsValuesV7,
            },
        };

        match value.version {
            GasCostsVersion::V1 => {
                // Distinguish V7 from V6 by presence of new storage fields
                if let Some(storage_read_cold) = value.storage_read_cold {
                    Ok(fuel_core_types::fuel_tx::GasCosts::new(
                        GasCostsValuesV7 {
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
                            niop: value.niop.map(Into::into).unwrap_or(0),
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
                            sub: value.sub.into(),
                            subi: value.subi.into(),
                            sw: value.sw.into(),
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
                            ecop: value.ecop.map(Into::into).unwrap_or(0),

                            aloc: value.aloc_dependent_cost.into(),
                            bsiz: value
                                .bsiz
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            bldd: value
                                .bldd
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
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
                            smo: value.smo.into(),
                            epar: value
                                .epar
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),

                            storage_read_cold: storage_read_cold.into(),
                            storage_read_hot: value
                                .storage_read_hot
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            storage_write: value
                                .storage_write
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            storage_clear: value
                                .storage_clear
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),

                            contract_root: value.contract_root.into(),
                            state_root: value.state_root.into(),
                            vm_initialization: value.vm_initialization.into(),
                            new_storage_per_byte: value.new_storage_per_byte.into(),
                        }
                        .into(),
                    ))
                } else {
                    Ok(fuel_core_types::fuel_tx::GasCosts::new(
                        GasCostsValuesV6 {
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
                            niop: value.niop.map(Into::into).unwrap_or(0),
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
                            srw: value.srw.map(Into::into).unwrap_or(0),
                            sub: value.sub.into(),
                            subi: value.subi.into(),
                            sw: value.sw.into(),
                            sww: value.sww.map(Into::into).unwrap_or(0),
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
                            ecop: value.ecop.map(Into::into).unwrap_or(0),

                            aloc: value.aloc_dependent_cost.into(),
                            bsiz: value
                                .bsiz
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            bldd: value
                                .bldd
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
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
                            scwq: value
                                .scwq
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            smo: value.smo.into(),
                            srwq: value
                                .srwq
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            swwq: value
                                .swwq
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            epar: value
                                .epar
                                .map(Into::into)
                                .unwrap_or(DependentCost::free()),
                            contract_root: value.contract_root.into(),
                            state_root: value.state_root.into(),
                            vm_initialization: value.vm_initialization.into(),
                            new_storage_per_byte: value.new_storage_per_byte.into(),
                        }
                        .into(),
                    ))
                }
            }
            _ => Err(ConversionError::UnknownVariant("GasCostsVersion")),
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
                fuel_core_types::fuel_tx::consensus_parameters::ConsensusParametersV2 {
                    tx_params: params.tx_params.try_into()?,
                    predicate_params: params.predicate_params.try_into()?,
                    script_params: params.script_params.try_into()?,
                    contract_params: params.contract_params.try_into()?,
                    fee_params: params.fee_params.try_into()?,
                    base_asset_id: params.base_asset_id.into(),
                    block_gas_limit: params.block_gas_limit.into(),
                    block_transaction_size_limit: params
                        .block_transaction_size_limit
                        .into(),
                    chain_id: params.chain_id.0.into(),
                    gas_costs: params.gas_costs.try_into()?,
                    privileged_address: params.privileged_address.into(),
                }
                .into(),
            ),
            _ => Err(ConversionError::UnknownVariant(
                "ConsensusParametersVersion",
            )),
        }
    }
}

/// Legacy `ScriptParameters` fragment for nodes older than v0.48.0 that do not expose
/// `maxStorageSlotLength`. Using this type causes cynic to omit that field from the
/// generated query, preventing "Unknown field" errors on old nodes.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "ScriptParameters")]
pub struct ScriptParametersLegacy {
    pub version: ScriptParametersVersion,
    pub max_script_length: U64,
    pub max_script_data_length: U64,
}

impl TryFrom<ScriptParametersLegacy> for fuel_core_types::fuel_tx::ScriptParameters {
    type Error = ConversionError;

    fn try_from(params: ScriptParametersLegacy) -> Result<Self, Self::Error> {
        // Pre-0.48.0 nodes only expose ScriptParameters V1 (no max_storage_slot_length).
        Ok(
            fuel_core_types::fuel_tx::consensus_parameters::ScriptParametersV1 {
                max_script_length: params.max_script_length.into(),
                max_script_data_length: params.max_script_data_length.into(),
            }
            .into(),
        )
    }
}

/// Legacy `GasCosts` fragment for nodes older than v0.48.0 that do not expose the
/// `storageReadCold`, `storageReadHot`, `storageWrite`, or `storageClear` fields.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "GasCosts")]
pub struct GasCostsLegacy {
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
    pub niop: Option<U64>,
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
    pub srw: Option<U64>,
    pub sub: U64,
    pub subi: U64,
    pub sw: U64,
    pub sww: Option<U64>,
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
    pub ecop: Option<U64>,

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
    pub scwq: Option<DependentCost>,
    pub smo: DependentCost,
    pub srwq: Option<DependentCost>,
    pub swwq: Option<DependentCost>,
    pub epar: Option<DependentCost>,

    // Non-opcodes prices
    pub contract_root: DependentCost,
    pub state_root: DependentCost,
    pub vm_initialization: DependentCost,
    pub new_storage_per_byte: U64,
}

impl TryFrom<GasCostsLegacy> for fuel_core_types::fuel_tx::GasCosts {
    type Error = ConversionError;

    fn try_from(value: GasCostsLegacy) -> Result<Self, Self::Error> {
        use fuel_core_types::fuel_tx::consensus_parameters::{
            DependentCost,
            gas::GasCostsValuesV6,
        };

        match value.version {
            GasCostsVersion::V1 => Ok(fuel_core_types::fuel_tx::GasCosts::new(
                GasCostsValuesV6 {
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
                    niop: value.niop.map(Into::into).unwrap_or(0),
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
                    srw: value.srw.map(Into::into).unwrap_or(0),
                    sub: value.sub.into(),
                    subi: value.subi.into(),
                    sw: value.sw.into(),
                    sww: value.sww.map(Into::into).unwrap_or(0),
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
                    ecop: value.ecop.map(Into::into).unwrap_or(0),

                    aloc: value.aloc_dependent_cost.into(),
                    bsiz: value.bsiz.map(Into::into).unwrap_or(DependentCost::free()),
                    bldd: value.bldd.map(Into::into).unwrap_or(DependentCost::free()),
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
                    scwq: value.scwq.map(Into::into).unwrap_or(DependentCost::free()),
                    smo: value.smo.into(),
                    srwq: value.srwq.map(Into::into).unwrap_or(DependentCost::free()),
                    swwq: value.swwq.map(Into::into).unwrap_or(DependentCost::free()),
                    epar: value.epar.map(Into::into).unwrap_or(DependentCost::free()),
                    contract_root: value.contract_root.into(),
                    state_root: value.state_root.into(),
                    vm_initialization: value.vm_initialization.into(),
                    new_storage_per_byte: value.new_storage_per_byte.into(),
                }
                .into(),
            )),
            _ => Err(ConversionError::UnknownVariant("GasCostsVersion")),
        }
    }
}

/// Legacy `ConsensusParameters` fragment for nodes older than v0.48.0.
/// Uses `ScriptParametersLegacy` and `GasCostsLegacy` to avoid requesting
/// fields that do not exist on old nodes.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "ConsensusParameters"
)]
pub struct ConsensusParametersLegacy {
    pub version: ConsensusParametersVersion,
    pub tx_params: TxParameters,
    pub predicate_params: PredicateParameters,
    pub script_params: ScriptParametersLegacy,
    pub contract_params: ContractParameters,
    pub fee_params: FeeParameters,
    pub base_asset_id: AssetId,
    pub block_gas_limit: U64,
    pub block_transaction_size_limit: U64,
    pub chain_id: U64,
    pub gas_costs: GasCostsLegacy,
    pub privileged_address: Address,
}

impl TryFrom<ConsensusParametersLegacy>
    for fuel_core_types::fuel_tx::ConsensusParameters
{
    type Error = ConversionError;

    fn try_from(params: ConsensusParametersLegacy) -> Result<Self, Self::Error> {
        match params.version {
            ConsensusParametersVersion::V1 => Ok(
                fuel_core_types::fuel_tx::consensus_parameters::ConsensusParametersV2 {
                    tx_params: params.tx_params.try_into()?,
                    predicate_params: params.predicate_params.try_into()?,
                    script_params: params.script_params.try_into()?,
                    contract_params: params.contract_params.try_into()?,
                    fee_params: params.fee_params.try_into()?,
                    base_asset_id: params.base_asset_id.into(),
                    block_gas_limit: params.block_gas_limit.into(),
                    block_transaction_size_limit: params
                        .block_transaction_size_limit
                        .into(),
                    chain_id: params.chain_id.0.into(),
                    gas_costs: params.gas_costs.try_into()?,
                    privileged_address: params.privileged_address.into(),
                }
                .into(),
            ),
            _ => Err(ConversionError::UnknownVariant(
                "ConsensusParametersVersion",
            )),
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

/// Legacy `ChainInfo` fragment for nodes older than v0.48.0.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "ChainInfo")]
pub struct ChainInfoLegacy {
    pub da_height: U64,
    pub name: String,
    pub latest_block: Block,
    pub consensus_parameters: ConsensusParametersLegacy,
}

/// Legacy chain query for nodes older than v0.48.0.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct ChainQueryLegacy {
    pub chain: ChainInfoLegacy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_gql_query_output() {
        use cynic::QueryBuilder;
        let operation = ChainQuery::build(());

        let snapshot_name = if cfg!(feature = "fault-proving") {
            "chain_gql_query_output_with_tx_id_commitment"
        } else {
            "chain_gql_query_output"
        };

        insta::assert_snapshot!(snapshot_name, operation.query)
    }

    /// Verifies that the legacy query does not request fields that don't exist on
    /// pre-0.48.0 nodes.  If `maxStorageSlotLength` or any of the `storage*Cost`
    /// fields ever sneak back into this query, the snapshot will fail loudly.
    #[test]
    fn chain_gql_legacy_query_output() {
        use cynic::QueryBuilder;
        let operation = ChainQueryLegacy::build(());

        let snapshot_name = if cfg!(feature = "fault-proving") {
            "chain_gql_legacy_query_output_with_tx_id_commitment"
        } else {
            "chain_gql_legacy_query_output"
        };

        insta::assert_snapshot!(snapshot_name, operation.query);

        // Belt-and-suspenders: assert the offending fields are absent so a reviewer
        // doesn't need to scan the whole snapshot by hand.
        assert!(
            !operation.query.contains("maxStorageSlotLength"),
            "legacy query must not request maxStorageSlotLength"
        );
        assert!(
            !operation.query.contains("storageReadCold"),
            "legacy query must not request storageReadCold"
        );
        assert!(
            !operation.query.contains("storageReadHot"),
            "legacy query must not request storageReadHot"
        );
        assert!(
            !operation.query.contains("storageWrite"),
            "legacy query must not request storageWrite"
        );
        assert!(
            !operation.query.contains("storageClear"),
            "legacy query must not request storageClear"
        );
    }
}
