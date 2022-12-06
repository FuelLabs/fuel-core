use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_tx::{
            Receipt,
            Transaction,
        },
    },
    executor::ExecutionResult,
    model::BlockHeight,
};

#[async_trait::async_trait]
pub trait BlockProducer: Send + Sync {
    // TODO: Right now production and execution of the block is one step, but in the future,
    //  `produce_block` should only produce a block without affecting the blockchain state.
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> anyhow::Result<ExecutionResult>;

    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>>;
}
