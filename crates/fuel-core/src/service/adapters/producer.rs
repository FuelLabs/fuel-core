use crate::{
    database::OnChainIterableKeyValueView,
    service::{
        adapters::{
            BlockProducerAdapter,
            ConsensusParametersProvider,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            StaticGasPrice,
            TransactionsSource,
            TxPoolAdapter,
        },
        sub_services::BlockProducerService,
    },
};
use fuel_core_executor::executor::OnceTransactionsSource;
use fuel_core_producer::{
    block_producer::gas_price::{
        ConsensusParametersProvider as ConsensusParametersProviderTrait,
        GasPriceProvider,
    },
    ports::TxPool,
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::{
        ConsensusParametersVersions,
        FuelBlocks,
        StateTransitionBytecodeVersions,
    },
    transactional::Changes,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx,
    fuel_tx::{
        ConsensusParameters,
        Transaction,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        block_producer::Components,
        executor::{
            Result as ExecutorResult,
            TransactionExecutionStatus,
            UncommittedResult,
        },
    },
};
use std::{
    borrow::Cow,
    sync::Arc,
};

impl BlockProducerAdapter {
    pub fn new(block_producer: BlockProducerService) -> Self {
        Self {
            block_producer: Arc::new(block_producer),
        }
    }
}

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    type TxSource = TransactionsSource;

    fn get_source(&self, block_height: BlockHeight) -> Self::TxSource {
        TransactionsSource::new(self.service.clone(), block_height)
    }
}

impl fuel_core_producer::ports::BlockProducer<TransactionsSource> for ExecutorAdapter {
    fn produce_without_commit(
        &self,
        component: Components<TransactionsSource>,
    ) -> ExecutorResult<UncommittedResult<Changes>> {
        self.executor.produce_without_commit_with_source(component)
    }
}

impl fuel_core_producer::ports::BlockProducer<Vec<Transaction>> for ExecutorAdapter {
    fn produce_without_commit(
        &self,
        component: Components<Vec<Transaction>>,
    ) -> ExecutorResult<UncommittedResult<Changes>> {
        let new_components = Components {
            header_to_produce: component.header_to_produce,
            transactions_source: OnceTransactionsSource::new(
                component.transactions_source,
            ),
            gas_price: component.gas_price,
            coinbase_recipient: component.coinbase_recipient,
        };

        self.executor
            .produce_without_commit_with_source(new_components)
    }
}

impl fuel_core_producer::ports::DryRunner for ExecutorAdapter {
    fn dry_run(
        &self,
        block: Components<Vec<fuel_tx::Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        self.executor.dry_run(block, utxo_validation)
    }
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Relayer for MaybeRelayerAdapter {
    async fn wait_for_at_least_height(
        &self,
        height: &DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_at_least_synced(height).await?;
                let highest = sync.get_finalized_da_height();
                Ok(highest)
            } else {
                Ok(*height)
            }
        }
        #[cfg(not(feature = "relayer"))]
        {
            anyhow::ensure!(
                **height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            // If the relayer is not enabled, then all blocks are zero.
            Ok(0u64.into())
        }
    }

    async fn get_cost_for_block(&self, height: &DaBlockHeight) -> anyhow::Result<u64> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                get_gas_cost_for_height(**height, sync)
            } else {
                Ok(0)
            }
        }
        #[cfg(not(feature = "relayer"))]
        {
            anyhow::ensure!(
                **height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            // If the relayer is not enabled, then all blocks are zero.
            Ok(0)
        }
    }
}

#[cfg(feature = "relayer")]
fn get_gas_cost_for_height(
    height: u64,
    sync: &fuel_core_relayer::SharedState<
        crate::database::Database<
            crate::database::database_description::relayer::Relayer,
        >,
    >,
) -> anyhow::Result<u64> {
    let da_height = DaBlockHeight(height);
    let cost = sync
        .database()
        .storage::<fuel_core_relayer::storage::EventsHistory>()
        .get(&da_height)?
        .unwrap_or_default()
        .iter()
        .fold(0u64, |acc, event| acc.saturating_add(event.cost()));
    Ok(cost)
}

impl fuel_core_producer::ports::BlockProducerDatabase for OnChainIterableKeyValueView {
    fn latest_height(&self) -> Option<BlockHeight> {
        self.latest_height().ok()
    }

    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>> {
        self.storage::<FuelBlocks>()
            .get(height)?
            .ok_or(not_found!(FuelBlocks))
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        self.storage::<FuelBlocks>().root(height).map(Into::into)
    }

    fn latest_consensus_parameters_version(
        &self,
    ) -> StorageResult<ConsensusParametersVersion> {
        let (version, _) = self
            .iter_all::<ConsensusParametersVersions>(Some(IterDirection::Reverse))
            .next()
            .ok_or(not_found!(ConsensusParametersVersions))??;

        Ok(version)
    }

    fn latest_state_transition_bytecode_version(
        &self,
    ) -> StorageResult<StateTransitionBytecodeVersion> {
        let (version, _) = self
            .iter_all::<StateTransitionBytecodeVersions>(Some(IterDirection::Reverse))
            .next()
            .ok_or(not_found!(StateTransitionBytecodeVersions))??;

        Ok(version)
    }
}

#[async_trait::async_trait]
impl GasPriceProvider for StaticGasPrice {
    async fn next_gas_price(&self) -> anyhow::Result<u64> {
        Ok(self.gas_price)
    }
}

impl ConsensusParametersProviderTrait for ConsensusParametersProvider {
    fn consensus_params_at_version(
        &self,
        version: &ConsensusParametersVersion,
    ) -> anyhow::Result<Arc<ConsensusParameters>> {
        Ok(self.shared_state.get_consensus_parameters(version)?)
    }
}
