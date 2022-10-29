use crate::{
    db::BlockProducerDatabase,
    ports::TxPool,
    Config,
};
use anyhow::{
    anyhow,
    Context,
    Result,
};
use chrono::Utc;
use fuel_core_interfaces::{
    block_producer::{
        BlockProducer as Trait,
        Error::{
            GenesisBlock,
            InvalidDaFinalizationState,
            MissingBlock,
        },
        Relayer,
    },
    common::{
        crypto::ephemeral_merkle_root,
        fuel_tx::{
            Receipt,
            Transaction,
            Word,
        },
        fuel_types::Bytes32,
    },
    executor::{
        Error,
        ExecutionBlock,
        Executor,
    },
    model::{
        BlockHeight,
        DaBlockHeight,
        FuelApplicationHeader,
        FuelBlock,
        FuelConsensusHeader,
        PartialFuelBlock,
        PartialFuelBlockHeader,
    },
};
use tokio::sync::Mutex;
use tracing::{
    debug,
    error,
};

#[cfg(test)]
mod tests;

pub struct Producer {
    pub config: Config,
    pub db: Box<dyn BlockProducerDatabase>,
    pub txpool: Box<dyn TxPool>,
    pub executor: Box<dyn Executor>,
    pub relayer: Box<dyn Relayer>,
    // use a tokio lock since we want callers to yield until the previous block
    // execution has completed (which may take a while).
    pub lock: Mutex<()>,
}

#[async_trait::async_trait]
impl Trait for Producer {
    /// Produces a block for the specified height
    async fn produce_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> Result<FuelBlock> {
        //  - get previous block info (hash, root, etc)
        //  - select best da_height from relayer
        //  - get available txs from txpool
        //  - select best txs based on factors like:
        //      1. fees
        //      2. parallel throughput
        //  - Execute block with production mode to correctly malleate txs outputs and block headers

        // prevent simultaneous block production calls, the guard will drop at the end of this fn.
        let _production_guard = self.lock.lock().await;

        let best_transactions = self.txpool.get_includable_txs(height, max_gas).await?;

        let header = self.new_header(height).await?;
        let block = PartialFuelBlock::new(
            header,
            best_transactions
                .into_iter()
                .map(|tx| tx.as_ref().into())
                .collect(),
        );

        // Store the context string incase we error.
        let context_string = format!(
            "Failed to produce block {:?} due to execution failure",
            block
        );
        let result = self
            .executor
            .execute(ExecutionBlock::Production(block))
            .await;

        if let Err(
            Error::VmExecution { transaction_id, .. }
            | Error::TransactionIdCollision(transaction_id),
        ) = &result
        {
            // TODO: if block execution fails due to any transaction validity errors,
            //          should those txs be removed from the txpool? While this
            //          theoretically shouldn't happen due to txpool validation rules,
            //          it is a possibility.
            error!(
                "faulty tx prevented block production: {:#x}",
                transaction_id
            );
        }

        let block = result.context(context_string)?;

        debug!("Produced block: {:?}", &block);
        Ok(block)
    }

    // simulate a transaction without altering any state. Does not aquire the production lock
    // since it is basically a "read only" operation and shouldn't get in the way of normal
    // production.
    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> Result<Vec<Receipt>> {
        // setup the block with the provided tx and optional height
        // dry_run execute tx on the executor
        // return the receipts

        let height = match height {
            None => self.db.current_block_height()?,
            Some(height) => height,
        } + 1u64.into();

        let is_script = transaction.is_script();
        let header = self.new_header(height).await?;
        let block =
            PartialFuelBlock::new(header, vec![transaction].into_iter().collect());

        let res: Vec<_> = self
            .executor
            .dry_run(ExecutionBlock::Production(block), utxo_validation)
            .await?
            .into_iter()
            .flatten()
            .collect();
        if is_script && res.is_empty() {
            return Err(anyhow!("Expected at least one set of receipts"))
        }
        Ok(res)
    }
}

impl Producer {
    /// Create the header for a new block at the provided height
    async fn new_header(&self, height: BlockHeight) -> Result<PartialFuelBlockHeader> {
        let previous_block_info = self.previous_block_info(height)?;
        let new_da_height = self
            .select_new_da_height(previous_block_info.da_height)
            .await?;

        Ok(PartialFuelBlockHeader {
            application: FuelApplicationHeader {
                da_height: new_da_height,
                generated: Default::default(),
            },
            consensus: FuelConsensusHeader {
                // TODO: this needs to be updated using a proper BMT MMR
                prev_root: previous_block_info.prev_root,
                height,
                time: Utc::now(),
                generated: Default::default(),
            },
            metadata: None,
        })
    }

    async fn select_new_da_height(
        &self,
        previous_da_height: DaBlockHeight,
    ) -> Result<DaBlockHeight> {
        let best_height = self.relayer.get_best_finalized_da_height().await?;
        if best_height < previous_da_height {
            // If this happens, it could mean a block was erroneously imported
            // without waiting for our relayer's da_height to catch up to imported da_height.
            return Err(InvalidDaFinalizationState {
                best: best_height,
                previous_block: previous_da_height,
            }
            .into())
        }
        Ok(best_height)
    }

    fn previous_block_info(&self, height: BlockHeight) -> Result<PreviousBlockInfo> {
        // block 0 is reserved for genesis
        if height == 0u32.into() {
            Err(GenesisBlock.into())
        }
        // if this is the first block, fill in base metadata from genesis
        else if height == 1u32.into() {
            // TODO: what should initial genesis data be here?
            Ok(PreviousBlockInfo {
                prev_root: Default::default(),
                da_height: Default::default(),
            })
        } else {
            // get info from previous block height
            let prev_height = height - 1u32.into();
            let previous_block = self
                .db
                .get_block(prev_height)?
                .ok_or(MissingBlock(prev_height))?;
            // TODO: this should use a proper BMT MMR
            let hash = previous_block.id();
            let prev_root = ephemeral_merkle_root(
                vec![*previous_block.header.prev_root(), hash.into()].iter(),
            );

            Ok(PreviousBlockInfo {
                prev_root,
                da_height: previous_block.header.da_height,
            })
        }
    }
}

struct PreviousBlockInfo {
    prev_root: Bytes32,
    da_height: DaBlockHeight,
}
