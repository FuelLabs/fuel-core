use crate::{
    fuel_core_graphql_api::{
        service::Database,
        Config as GraphQLConfig,
    },
    query::{
        BlockQueryData,
        ChainQueryData,
    },
    schema::{
        block::Block,
        scalars::{
            U32,
            U64,
        },
    },
};
use async_graphql::{
    Context,
    Object,
};
use fuel_core_types::{
    fuel_tx,
    fuel_vm,
};

pub struct ChainInfo;

pub struct ConsensusParameters(fuel_tx::ConsensusParameters);

pub struct GasCosts(fuel_vm::GasCosts);

pub struct DependentCost(fuel_vm::DependentCost);

impl From<fuel_vm::DependentCost> for DependentCost {
    fn from(value: fuel_vm::DependentCost) -> Self {
        Self(value)
    }
}

#[Object]
impl ConsensusParameters {
    async fn contract_max_size(&self) -> U64 {
        self.0.contract_max_size.into()
    }

    async fn max_inputs(&self) -> U64 {
        self.0.max_inputs.into()
    }

    async fn max_outputs(&self) -> U64 {
        self.0.max_outputs.into()
    }

    async fn max_witnesses(&self) -> U64 {
        self.0.max_witnesses.into()
    }

    async fn max_gas_per_tx(&self) -> U64 {
        self.0.max_gas_per_tx.into()
    }

    async fn max_script_length(&self) -> U64 {
        self.0.max_script_length.into()
    }

    async fn max_script_data_length(&self) -> U64 {
        self.0.max_script_data_length.into()
    }

    async fn max_storage_slots(&self) -> U64 {
        self.0.max_storage_slots.into()
    }

    async fn max_predicate_length(&self) -> U64 {
        self.0.max_predicate_length.into()
    }

    async fn max_predicate_data_length(&self) -> U64 {
        self.0.max_predicate_data_length.into()
    }

    async fn max_gas_per_predicate(&self) -> U64 {
        self.0.max_gas_per_predicate.into()
    }

    async fn gas_price_factor(&self) -> U64 {
        self.0.gas_price_factor.into()
    }

    async fn gas_per_byte(&self) -> U64 {
        self.0.gas_per_byte.into()
    }

    async fn max_message_data_length(&self) -> U64 {
        self.0.max_message_data_length.into()
    }

    async fn chain_id(&self) -> U64 {
        (*self.0.chain_id).into()
    }
}

#[Object]
impl GasCosts {
    async fn add(&self) -> U64 {
        self.0.add.into()
    }

    async fn addi(&self) -> U64 {
        self.0.addi.into()
    }

    async fn aloc(&self) -> U64 {
        self.0.aloc.into()
    }

    async fn and(&self) -> U64 {
        self.0.and.into()
    }

    async fn andi(&self) -> U64 {
        self.0.andi.into()
    }

    async fn bal(&self) -> U64 {
        self.0.bal.into()
    }

    async fn bhei(&self) -> U64 {
        self.0.bhei.into()
    }

    async fn bhsh(&self) -> U64 {
        self.0.bhsh.into()
    }

    async fn burn(&self) -> U64 {
        self.0.burn.into()
    }

    async fn cb(&self) -> U64 {
        self.0.cb.into()
    }

    async fn cfei(&self) -> U64 {
        self.0.cfei.into()
    }

    async fn cfsi(&self) -> U64 {
        self.0.cfsi.into()
    }

    async fn croo(&self) -> U64 {
        self.0.croo.into()
    }

    async fn div(&self) -> U64 {
        self.0.div.into()
    }

    async fn divi(&self) -> U64 {
        self.0.divi.into()
    }

    async fn ecr(&self) -> U64 {
        self.0.ecr.into()
    }

    async fn eq(&self) -> U64 {
        self.0.eq.into()
    }

    async fn exp(&self) -> U64 {
        self.0.exp.into()
    }

    async fn expi(&self) -> U64 {
        self.0.expi.into()
    }

    async fn flag(&self) -> U64 {
        self.0.flag.into()
    }

    async fn gm(&self) -> U64 {
        self.0.gm.into()
    }

    async fn gt(&self) -> U64 {
        self.0.gt.into()
    }

    async fn gtf(&self) -> U64 {
        self.0.gtf.into()
    }

    async fn ji(&self) -> U64 {
        self.0.ji.into()
    }

    async fn jmp(&self) -> U64 {
        self.0.jmp.into()
    }

    async fn jne(&self) -> U64 {
        self.0.jne.into()
    }

    async fn jnei(&self) -> U64 {
        self.0.jnei.into()
    }

    async fn jnzi(&self) -> U64 {
        self.0.jnzi.into()
    }

    async fn jmpf(&self) -> U64 {
        self.0.jmpf.into()
    }

    async fn jmpb(&self) -> U64 {
        self.0.jmpb.into()
    }

    async fn jnzf(&self) -> U64 {
        self.0.jnzf.into()
    }

    async fn jnzb(&self) -> U64 {
        self.0.jnzb.into()
    }

    async fn jnef(&self) -> U64 {
        self.0.jnef.into()
    }

    async fn jneb(&self) -> U64 {
        self.0.jneb.into()
    }

    async fn k256(&self) -> U64 {
        self.0.k256.into()
    }

    async fn lb(&self) -> U64 {
        self.0.lb.into()
    }

    async fn log(&self) -> U64 {
        self.0.log.into()
    }

    async fn lt(&self) -> U64 {
        self.0.lt.into()
    }

    async fn lw(&self) -> U64 {
        self.0.lw.into()
    }

    async fn mcpi(&self) -> U64 {
        self.0.mcpi.into()
    }

    async fn mint(&self) -> U64 {
        self.0.mint.into()
    }

    async fn mlog(&self) -> U64 {
        self.0.mlog.into()
    }

    async fn mod_op(&self) -> U64 {
        self.0.mod_op.into()
    }

    async fn modi(&self) -> U64 {
        self.0.modi.into()
    }

    async fn move_op(&self) -> U64 {
        self.0.move_op.into()
    }

    async fn movi(&self) -> U64 {
        self.0.movi.into()
    }

    async fn mroo(&self) -> U64 {
        self.0.mroo.into()
    }

    async fn mul(&self) -> U64 {
        self.0.mul.into()
    }

    async fn muli(&self) -> U64 {
        self.0.muli.into()
    }

    async fn mldv(&self) -> U64 {
        self.0.mldv.into()
    }

    async fn noop(&self) -> U64 {
        self.0.noop.into()
    }

    async fn not(&self) -> U64 {
        self.0.not.into()
    }

    async fn or(&self) -> U64 {
        self.0.or.into()
    }

    async fn ori(&self) -> U64 {
        self.0.ori.into()
    }

    async fn ret(&self) -> U64 {
        self.0.ret.into()
    }

    async fn rvrt(&self) -> U64 {
        self.0.rvrt.into()
    }

    async fn s256(&self) -> U64 {
        self.0.s256.into()
    }

    async fn sb(&self) -> U64 {
        self.0.sb.into()
    }

    async fn scwq(&self) -> U64 {
        self.0.scwq.into()
    }

    async fn sll(&self) -> U64 {
        self.0.sll.into()
    }

    async fn slli(&self) -> U64 {
        self.0.slli.into()
    }

    async fn srl(&self) -> U64 {
        self.0.srl.into()
    }

    async fn srli(&self) -> U64 {
        self.0.srli.into()
    }

    async fn srw(&self) -> U64 {
        self.0.srw.into()
    }

    async fn sub(&self) -> U64 {
        self.0.sub.into()
    }

    async fn subi(&self) -> U64 {
        self.0.subi.into()
    }

    async fn sw(&self) -> U64 {
        self.0.sw.into()
    }

    async fn sww(&self) -> U64 {
        self.0.sww.into()
    }

    async fn swwq(&self) -> U64 {
        self.0.swwq.into()
    }

    async fn time(&self) -> U64 {
        self.0.time.into()
    }

    async fn tr(&self) -> U64 {
        self.0.tr.into()
    }

    async fn tro(&self) -> U64 {
        self.0.tro.into()
    }

    async fn wdcm(&self) -> U64 {
        self.0.wdcm.into()
    }

    async fn wqcm(&self) -> U64 {
        self.0.wqcm.into()
    }

    async fn wdop(&self) -> U64 {
        self.0.wdop.into()
    }

    async fn wqop(&self) -> U64 {
        self.0.wqop.into()
    }

    async fn wdml(&self) -> U64 {
        self.0.wdml.into()
    }

    async fn wqml(&self) -> U64 {
        self.0.wqml.into()
    }

    async fn wddv(&self) -> U64 {
        self.0.wddv.into()
    }

    async fn wqdv(&self) -> U64 {
        self.0.wqdv.into()
    }

    async fn wdmd(&self) -> U64 {
        self.0.wdmd.into()
    }

    async fn wqmd(&self) -> U64 {
        self.0.wqmd.into()
    }

    async fn wdam(&self) -> U64 {
        self.0.wdam.into()
    }

    async fn wqam(&self) -> U64 {
        self.0.wqam.into()
    }

    async fn wdmm(&self) -> U64 {
        self.0.wdmm.into()
    }

    async fn wqmm(&self) -> U64 {
        self.0.wqmm.into()
    }

    async fn xor(&self) -> U64 {
        self.0.xor.into()
    }

    async fn xori(&self) -> U64 {
        self.0.xori.into()
    }

    async fn call(&self) -> DependentCost {
        self.0.call.into()
    }

    async fn ccp(&self) -> DependentCost {
        self.0.ccp.into()
    }

    async fn csiz(&self) -> DependentCost {
        self.0.csiz.into()
    }

    async fn ldc(&self) -> DependentCost {
        self.0.ldc.into()
    }

    async fn logd(&self) -> DependentCost {
        self.0.logd.into()
    }

    async fn mcl(&self) -> DependentCost {
        self.0.mcl.into()
    }

    async fn mcli(&self) -> DependentCost {
        self.0.mcli.into()
    }

    async fn mcp(&self) -> DependentCost {
        self.0.mcp.into()
    }

    async fn meq(&self) -> DependentCost {
        self.0.meq.into()
    }

    async fn retd(&self) -> DependentCost {
        self.0.retd.into()
    }

    async fn smo(&self) -> DependentCost {
        self.0.smo.into()
    }

    async fn srwq(&self) -> DependentCost {
        self.0.srwq.into()
    }
}

#[Object]
impl DependentCost {
    async fn base(&self) -> U64 {
        self.0.base.into()
    }

    async fn dep_per_unit(&self) -> U64 {
        self.0.dep_per_unit.into()
    }
}

#[Object]
impl ChainInfo {
    async fn name(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let data: &Database = ctx.data_unchecked();
        Ok(data.name()?)
    }

    async fn latest_block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query: &Database = ctx.data_unchecked();

        let latest_block = query.latest_block()?.into();
        Ok(latest_block)
    }

    async fn base_chain_height(&self, ctx: &Context<'_>) -> U32 {
        let query: &Database = ctx.data_unchecked();

        let height = query
            .latest_block_height()
            .expect("The blockchain always should have genesis block");

        height.into()
    }

    async fn peer_count(&self) -> u16 {
        0
    }

    async fn consensus_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ConsensusParameters> {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        Ok(ConsensusParameters(config.transaction_parameters))
    }

    async fn gas_costs(&self, ctx: &Context<'_>) -> async_graphql::Result<GasCosts> {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        Ok(GasCosts(config.gas_costs.clone()))
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
