use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_tx::{
            Receipt,
            Transaction,
        },
    },
    db::DatabaseTransaction,
    executor::UncommittedResult,
    model::{
        BlockHeight,
        BlockId,
        FuelBlockConsensus,
    },
};

pub trait BlockDb: Send + Sync {
    fn block_height(&self) -> anyhow::Result<BlockHeight>;

    // Returns error if already sealed
    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: FuelBlockConsensus,
    ) -> anyhow::Result<()>;
}

// TODO: Replace by the analog from the `fuel-core-storage`.
pub type DBTransaction<Database> = Box<dyn DatabaseTransaction<Database>>;

#[async_trait::async_trait]
pub trait BlockProducer<Database>: Send + Sync {
    // TODO: Right now production and execution of the block is one step, but in the future,
    //  `produce_block` should only produce a block without affecting the blockchain state.
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<DBTransaction<Database>>>;

    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>>;
}
