use crate::{
    block_producer::gas_price::{
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderConstraint,
    },
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
    Changes,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx::{
        field::{
            InputContract,
            MintGasPrice,
        },
        Transaction,
    },
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

pub mod gas_price;

#[derive(Debug, derive_more::Display)]
pub enum Error {
    #[display(fmt = "Genesis block is absent")]
    NoGenesisBlock,
    #[display(
        fmt = "The block height {height} should be higher than the previous block height {previous_block}"
    )]
    BlockHeightShouldBeHigherThanPrevious {
        height: BlockHeight,
        previous_block: BlockHeight,
    },
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

pub struct Producer<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider> {
    pub config: Config,
    pub view_provider: ViewProvider,
    pub txpool: TxPool,
    pub executor: Arc<Executor>,
    pub relayer: Box<dyn ports::Relayer>,
    // use a tokio lock since we want callers to yield until the previous block
    // execution has completed (which may take a while).
    pub lock: Mutex<()>,
    pub gas_price_provider: GasPriceProvider,
    pub consensus_parameters_provider: ConsensusProvider,
}

impl<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
    Producer<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::LatestView: BlockProducerDatabase,
    ConsensusProvider: ConsensusParametersProvider,
{
    pub async fn produce_and_execute_predefined(
        &self,
        predefined_block: &Block,
    ) -> anyhow::Result<UncommittedResult<Changes>>
    where
        Executor: ports::BlockProducer<Vec<Transaction>> + 'static,
    {
        let _production_guard = self.lock.lock().await;

        let mut transactions_source = predefined_block.transactions().to_vec();

        let height = predefined_block.header().consensus().height;

        let block_time = predefined_block.header().consensus().time;

        let da_height = predefined_block.header().application().da_height;

        let header_to_produce =
            self.new_header_with_da_height(height, block_time, da_height)?;

        let maybe_mint_tx = transactions_source.pop();
        let mint_tx =
            maybe_mint_tx
                .and_then(|tx| tx.as_mint().cloned())
                .ok_or(anyhow!(
                    "The last transaction in the block should be a mint transaction"
                ))?;

        let gas_price = *mint_tx.gas_price();
        let coinbase_recipient = mint_tx.input_contract().contract_id;

        let component = Components {
            header_to_produce,
            transactions_source,
            coinbase_recipient,
            gas_price,
        };

        let result = self
            .executor
            .produce_without_commit(component)
            .map_err(Into::<anyhow::Error>::into)
            .with_context(|| {
                format!("Failed to produce block {height:?} due to execution failure")
            })?;

        debug!("Produced block with result: {:?}", result.result());
        Ok(result)
    }
}
impl<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
    Producer<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::LatestView: BlockProducerDatabase,
    GasPriceProvider: GasPriceProviderConstraint,
    ConsensusProvider: ConsensusParametersProvider,
{
    /// Produces and execute block for the specified height.
    async fn produce_and_execute<TxSource>(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        tx_source: impl FnOnce(BlockHeight) -> TxSource,
    ) -> anyhow::Result<UncommittedResult<Changes>>
    where
        Executor: ports::BlockProducer<TxSource> + 'static,
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

        let gas_price = self.calculate_gas_price().await?;

        let component = Components {
            header_to_produce: header,
            transactions_source: source,
            coinbase_recipient: self.config.coinbase_recipient.unwrap_or_default(),
            gas_price,
        };

        // Store the context string in case we error.
        let context_string =
            format!("Failed to produce block {height:?} due to execution failure");
        let result = self
            .executor
            .produce_without_commit(component)
            .map_err(Into::<anyhow::Error>::into)
            .context(context_string)?;

        debug!("Produced block with result: {:?}", result.result());
        Ok(result)
    }

    async fn calculate_gas_price(&self) -> anyhow::Result<u64> {
        self.gas_price_provider
            .next_gas_price()
            .await
            .map_err(|e| anyhow!("No gas price found: {e:?}"))
    }
}

impl<ViewProvider, TxPool, Executor, TxSource, GasPriceProvider, ConsensusProvider>
    Producer<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::LatestView: BlockProducerDatabase,
    TxPool: ports::TxPool<TxSource = TxSource> + 'static,
    Executor: ports::BlockProducer<TxSource> + 'static,
    GasPriceProvider: GasPriceProviderConstraint,
    ConsensusProvider: ConsensusParametersProvider,
{
    /// Produces and execute block for the specified height with transactions from the `TxPool`.
    pub async fn produce_and_execute_block_txpool(
        &self,
        height: BlockHeight,
        block_time: Tai64,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        self.produce_and_execute(height, block_time, |height| {
            self.txpool.get_source(height)
        })
        .await
    }
}

impl<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
    Producer<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::LatestView: BlockProducerDatabase,
    Executor: ports::BlockProducer<Vec<Transaction>> + 'static,
    GasPriceProvider: GasPriceProviderConstraint,
    ConsensusProvider: ConsensusParametersProvider,
{
    /// Produces and execute block for the specified height with `transactions`.
    pub async fn produce_and_execute_block_transactions(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        transactions: Vec<Transaction>,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        self.produce_and_execute(height, block_time, |_| transactions)
            .await
    }
}

impl<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
    Producer<ViewProvider, TxPool, Executor, GasPriceProvider, ConsensusProvider>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::LatestView: BlockProducerDatabase,
    Executor: ports::DryRunner + 'static,
    GasPriceProvider: GasPriceProviderConstraint,
    ConsensusProvider: ConsensusParametersProvider,
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
        gas_price: Option<u64>,
    ) -> anyhow::Result<Vec<TransactionExecutionStatus>> {
        let view = self.view_provider.latest_view()?;
        let height = height.unwrap_or_else(|| {
            view.latest_height()
                .unwrap_or_default()
                .succ()
                .expect("It is impossible to overflow the current block height")
        });

        let header = self._new_header(height, Tai64::now())?;

        let gas_price = if let Some(inner) = gas_price {
            inner
        } else {
            self.calculate_gas_price().await?
        };

        // The dry run execution should use the state of the blockchain based on the
        // last available block, not on the upcoming one. It means that we need to
        // use the same configuration as the last block -> the same DA height.
        // It is deterministic from the result perspective, plus it is more performant
        // because we don't need to wait for the relayer to sync.
        let component = Components {
            header_to_produce: header,
            transactions_source: transactions.clone(),
            coinbase_recipient: self.config.coinbase_recipient.unwrap_or_default(),
            gas_price,
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

pub const NO_NEW_DA_HEIGHT_FOUND: &str = "No new da_height found";

impl<ViewProvider, TxPool, Executor, GP, ConsensusProvider>
    Producer<ViewProvider, TxPool, Executor, GP, ConsensusProvider>
where
    ViewProvider: AtomicView + 'static,
    ViewProvider::LatestView: BlockProducerDatabase,
    ConsensusProvider: ConsensusParametersProvider,
{
    /// Create the header for a new block at the provided height
    async fn new_header(
        &self,
        height: BlockHeight,
        block_time: Tai64,
    ) -> anyhow::Result<PartialBlockHeader> {
        let mut block_header = self._new_header(height, block_time)?;
        let previous_da_height = block_header.da_height;
        let gas_limit = self
            .consensus_parameters_provider
            .consensus_params_at_version(&block_header.consensus_parameters_version)?
            .block_gas_limit();
        let new_da_height = self
            .select_new_da_height(gas_limit, previous_da_height)
            .await?;

        block_header.application.da_height = new_da_height;

        Ok(block_header)
    }
    /// Create the header for a new block at the provided height
    fn new_header_with_da_height(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        da_height: DaBlockHeight,
    ) -> anyhow::Result<PartialBlockHeader> {
        let mut block_header = self._new_header(height, block_time)?;
        block_header.application.da_height = da_height;
        Ok(block_header)
    }
    async fn select_new_da_height(
        &self,
        gas_limit: u64,
        previous_da_height: DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight> {
        let mut new_best = previous_da_height;
        let mut total_cost: u64 = 0;
        let highest = self
            .relayer
            .wait_for_at_least_height(&previous_da_height)
            .await?;
        if highest < previous_da_height {
            return Err(Error::InvalidDaFinalizationState {
                best: highest,
                previous_block: previous_da_height,
            }
            .into());
        }

        if highest == previous_da_height {
            return Ok(highest);
        }

        let next_da_height = previous_da_height.saturating_add(1);
        for height in next_da_height..=highest.0 {
            let cost = self
                .relayer
                .get_cost_for_block(&DaBlockHeight(height))
                .await?;
            total_cost = total_cost.saturating_add(cost);
            if total_cost > gas_limit {
                break;
            } else {
                new_best = DaBlockHeight(height);
            }
        }
        if new_best == previous_da_height {
            Err(anyhow!(NO_NEW_DA_HEIGHT_FOUND))
        } else {
            Ok(new_best)
        }
    }

    fn _new_header(
        &self,
        height: BlockHeight,
        block_time: Tai64,
    ) -> anyhow::Result<PartialBlockHeader> {
        let view = self.view_provider.latest_view()?;
        let previous_block_info = self.previous_block_info(height, &view)?;
        let consensus_parameters_version = view.latest_consensus_parameters_version()?;
        let state_transition_bytecode_version =
            view.latest_state_transition_bytecode_version()?;

        Ok(PartialBlockHeader {
            application: ApplicationHeader {
                da_height: previous_block_info.da_height,
                consensus_parameters_version,
                state_transition_bytecode_version,
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
        view: &ViewProvider::LatestView,
    ) -> anyhow::Result<PreviousBlockInfo> {
        let latest_height = self
            .view_provider
            .latest_view()?
            .latest_height()
            .ok_or(Error::NoGenesisBlock)?;
        // block 0 is reserved for genesis
        if height <= latest_height {
            Err(Error::BlockHeightShouldBeHigherThanPrevious {
                height,
                previous_block: latest_height,
            }
            .into())
        } else {
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
