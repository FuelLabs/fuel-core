use crate::{
    ports,
    ports::BlockProducerDatabase,
    Config,
};
use anyhow::{
    anyhow,
    Context,
};
use fuel_core_storage::transactional::{
    AtomicView,
    StorageTransaction,
};
use fuel_core_types::{
    blockchain::{
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::DaBlockHeight,
    },
    fuel_asm::Word,
    fuel_tx::Transaction,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        block_producer::Components,
        executor::{
            TransactionExecutionStatus,
            UncommittedResult,
        },
    },
    tai64::Tai64,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

#[cfg(test)]
mod tests;

#[derive(Debug, derive_more::Display)]
pub enum Error {
    #[display(
        fmt = "0 is an invalid block height for production. It is reserved for genesis data."
    )]
    GenesisBlock,
    #[display(fmt = "Previous block height {_0} doesn't exist")]
    MissingBlock(BlockHeight),
    #[display(
        fmt = "Best finalized da_height {best} is behind previous block da_height {previous_block}"
    )]
    InvalidDaFinalizationState {
        best: DaBlockHeight,
        previous_block: DaBlockHeight,
    },
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

pub struct Producer<ViewProvider, TxPool, Executor> {
    pub config: Config,
    pub view_provider: ViewProvider,
    pub txpool: TxPool,
    pub executor: Arc<Executor>,
    pub relayer: Box<dyn ports::Relayer>,
    // use a tokio lock since we want callers to yield until the previous block
    // execution has completed (which may take a while).
    pub lock: Mutex<()>,
}

impl<ViewProvider, TxPool, Executor> Producer<ViewProvider, TxPool, Executor>
where
    ViewProvider: AtomicView<Height = BlockHeight> + 'static,
    ViewProvider::View: BlockProducerDatabase,
{
    /// Produces and execute block for the specified height.
    async fn produce_and_execute<TxSource, ExecutorDB>(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        tx_source: impl FnOnce(BlockHeight) -> TxSource,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<ExecutorDB>>>
    where
        Executor: ports::Executor<TxSource, Database = ExecutorDB> + 'static,
    {
        //  - get previous block info (hash, root, etc)
        //  - select best da_height from relayer
        //  - get available txs from txpool
        //  - select best txs based on factors like:
        //      1. fees
        //      2. parallel throughput
        //  - Execute block with production mode to correctly malleate txs outputs and block headers

        // prevent simultaneous block production calls, the guard will drop at the end of this fn.
        let _production_guard = self.lock.lock().await;

        let source = tx_source(height);

        let header = self.new_header(height, block_time).await?;

        let component = Components {
            header_to_produce: header,
            transactions_source: source,
            gas_limit: max_gas,
        };

        // Store the context string in case we error.
        let context_string =
            format!("Failed to produce block {height:?} due to execution failure");
        let result = self
            .executor
            .execute_without_commit(component)
            .map_err(Into::<anyhow::Error>::into)
            .context(context_string)?;

        debug!("Produced block with result: {:?}", result.result());
        Ok(result)
    }
}

impl<ViewProvider, TxPool, Executor, ExecutorDB, TxSource>
    Producer<ViewProvider, TxPool, Executor>
where
    ViewProvider: AtomicView<Height = BlockHeight> + 'static,
    ViewProvider::View: BlockProducerDatabase,
    TxPool: ports::TxPool<TxSource = TxSource> + 'static,
    Executor: ports::Executor<TxSource, Database = ExecutorDB> + 'static,
{
    /// Produces and execute block for the specified height with transactions from the `TxPool`.
    pub async fn produce_and_execute_block_txpool(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<ExecutorDB>>> {
        self.produce_and_execute(
            height,
            block_time,
            |height| self.txpool.get_source(height),
            max_gas,
        )
        .await
    }
}

impl<ViewProvider, TxPool, Executor, ExecutorDB> Producer<ViewProvider, TxPool, Executor>
where
    ViewProvider: AtomicView<Height = BlockHeight> + 'static,
    ViewProvider::View: BlockProducerDatabase,
    Executor: ports::Executor<Vec<Transaction>, Database = ExecutorDB> + 'static,
{
    /// Produces and execute block for the specified height with `transactions`.
    pub async fn produce_and_execute_block_transactions(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        transactions: Vec<Transaction>,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<ExecutorDB>>> {
        self.produce_and_execute(height, block_time, |_| transactions, max_gas)
            .await
    }
}

impl<ViewProvider, TxPool, Executor> Producer<ViewProvider, TxPool, Executor>
where
    ViewProvider: AtomicView<Height = BlockHeight> + 'static,
    ViewProvider::View: BlockProducerDatabase,
    Executor: ports::DryRunner + 'static,
{
    // TODO: Support custom `block_time` for `dry_run`.
    /// Simulates multiple transactions without altering any state. Does not acquire the production lock.
    /// since it is basically a "read only" operation and shouldn't get in the way of normal
    /// production.
    pub async fn dry_run(
        &self,
        transactions: Vec<Transaction>,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<TransactionExecutionStatus>> {
        let height = height.unwrap_or_else(|| {
            self.view_provider
                .latest_height()
                .succ()
                .expect("It is impossible to overflow the current block height")
        });

        // The dry run execution should use the state of the blockchain based on the
        // last available block, not on the upcoming one. It means that we need to
        // use the same configuration as the last block -> the same DA height.
        // It is deterministic from the result perspective, plus it is more performant
        // because we don't need to wait for the relayer to sync.
        let header = self._new_header(height, Tai64::now())?;
        let component = Components {
            header_to_produce: header,
            transactions_source: transactions.clone(),
            gas_limit: u64::MAX,
        };

        let executor = self.executor.clone();

        // use the blocking threadpool for dry_run to avoid clogging up the main async runtime
        let tx_statuses = tokio_rayon::spawn_fifo(
            move || -> anyhow::Result<Vec<TransactionExecutionStatus>> {
                Ok(executor.dry_run(component, utxo_validation)?)
            },
        )
        .await?;

        if transactions
            .iter()
            .zip(tx_statuses.iter())
            .any(|(transaction, tx_status)| {
                transaction.is_script() && tx_status.result.receipts().is_empty()
            })
        {
            Err(anyhow!("Expected at least one set of receipts"))
        } else {
            Ok(tx_statuses)
        }
    }
}

impl<ViewProvider, TxPool, Executor> Producer<ViewProvider, TxPool, Executor>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::View: BlockProducerDatabase,
{
    /// Create the header for a new block at the provided height
    async fn new_header(
        &self,
        height: BlockHeight,
        block_time: Tai64,
    ) -> anyhow::Result<PartialBlockHeader> {
        let mut block_header = self._new_header(height, block_time)?;
        let new_da_height = self.select_new_da_height(block_header.da_height).await?;

        block_header.application.da_height = new_da_height;

        Ok(block_header)
    }

    async fn select_new_da_height(
        &self,
        previous_da_height: DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight> {
        let best_height = self.relayer.wait_for_at_least(&previous_da_height).await?;
        if best_height < previous_da_height {
            // If this happens, it could mean a block was erroneously imported
            // without waiting for our relayer's da_height to catch up to imported da_height.
            return Err(Error::InvalidDaFinalizationState {
                best: best_height,
                previous_block: previous_da_height,
            }
            .into())
        }
        Ok(best_height)
    }

    fn _new_header(
        &self,
        height: BlockHeight,
        block_time: Tai64,
    ) -> anyhow::Result<PartialBlockHeader> {
        let previous_block_info = self.previous_block_info(height)?;

        Ok(PartialBlockHeader {
            application: ApplicationHeader {
                da_height: previous_block_info.da_height,
                generated: Default::default(),
            },
            consensus: ConsensusHeader {
                prev_root: previous_block_info.prev_root,
                height,
                time: block_time,
                generated: Default::default(),
            },
        })
    }

    fn previous_block_info(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<PreviousBlockInfo> {
        // TODO: It is not guaranteed that the genesis height is `0` height. Update the code to
        //  use a genesis height from the database. If the `height` less than genesis height ->
        //  return a new error.
        // block 0 is reserved for genesis
        if height == 0u32.into() {
            Err(Error::GenesisBlock.into())
        } else {
            let view = self.view_provider.latest_view();
            // get info from previous block height
            let prev_height = height.pred().expect("We checked the height above");
            let previous_block = view.get_block(&prev_height)?;
            let prev_root = view.block_header_merkle_root(&prev_height)?;

            Ok(PreviousBlockInfo {
                prev_root,
                da_height: previous_block.header().da_height,
            })
        }
    }
}

struct PreviousBlockInfo {
    prev_root: Bytes32,
    da_height: DaBlockHeight,
}
