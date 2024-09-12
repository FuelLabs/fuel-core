use std::sync::Arc;

use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    blockchain::consensus::Consensus,
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::txpool::PoolTransaction,
};
use parking_lot::RwLock;

use crate::{
    collision_manager::basic::BasicCollisionManager, config::Config, error::Error, pool::Pool, ports::{
        AtomicView,
        ConsensusParametersProvider,
        TxPoolPersistentStorage,
    }, selection_algorithms::ratio_tip_gas::RatioTipGasSelection, storage::graph::{
        GraphConfig, GraphStorage, GraphStorageIndex
    }, transaction_conversion::check_transactions
};

pub struct SharedState<PSProvider, ConsensusParamsProvider> {
    pool: Arc<
        RwLock<
            Pool<
                PSProvider,
                GraphStorage,
                BasicCollisionManager<GraphStorageIndex>,
                RatioTipGasSelection<GraphStorageIndex>,
            >,
        >,
    >,
    current_height: Arc<RwLock<BlockHeight>>,
    consensus_parameters_provider: Arc<ConsensusParamsProvider>,
}

impl<PSProvider, ConsensusParamsProvider> Clone
    for SharedState<PSProvider, ConsensusParamsProvider>
{
    fn clone(&self) -> Self {
        SharedState {
            pool: self.pool.clone(),
            current_height: self.current_height.clone(),
            consensus_parameters_provider: self.consensus_parameters_provider.clone(),
        }
    }
}

impl<PSProvider, PSView, ConsensusParamsProvider>
    SharedState<PSProvider, ConsensusParamsProvider>
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider,
{
    // TODO: Implement conversion from `Transaction` to `PoolTransaction`. (with all the verifications that it implies): https://github.com/FuelLabs/fuel-core/issues/2186
    async fn insert(
        &mut self,
        transactions: Vec<Transaction>,
    ) -> Result<Vec<Result<Vec<PoolTransaction>, Error>>, Error> {
        let current_height = *self.current_height.read();
        let (version, params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();

        let checked_txs = check_transactions(
            transactions,
            current_height,
            self.utxo_validation,
            params.as_ref(),
            &self.gas_price_provider,
            self.memory_pool.clone(),
        )
        .await;

        let mut valid_txs = vec![];

        let checked_txs: Vec<_> = checked_txs
            .into_iter()
            .map(|tx_check| match tx_check {
                Ok(tx) => {
                    valid_txs.push(tx);
                    None
                }
                Err(err) => Some(err),
            })
            .collect();
        // Move verif of wasm there

        self.pool.insert(transactions)
    }

    pub fn find_one(&self, tx_id: &TxId) -> Option<Transaction> {
        self.pool.find_one(tx_id).map(|tx| tx.into())
    }
}

pub type Service<PSProvider, ConsensusParamsProvider> =
    ServiceRunner<Task<PSProvider, ConsensusParamsProvider>>;

pub struct Task<PSProvider, ConsensusParamsProvider> {
    shared_state: SharedState<PSProvider, ConsensusParamsProvider>,
}

#[async_trait::async_trait]
impl<PSProvider, PSView, ConsensusParamsProvider> RunnableService
    for Task<PSProvider, ConsensusParamsProvider>
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
{
    const NAME: &'static str = "TxPoolv2";

    type SharedData = SharedState<PSProvider, ConsensusParamsProvider>;

    type Task = Task<PSProvider, ConsensusParamsProvider>;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<PSProvider, PSView, ConsensusParamsProvider> RunnableTask
    for Task<PSProvider, ConsensusParamsProvider>
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            _ = watcher.while_started() => {
                should_continue = false;
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn new_service<PSProvider, PSView, ConsensusParamsProvider>(
    ps_provider: PSProvider,
    consensus_parameters_provider: ConsensusParamsProvider,
    current_height: BlockHeight,
    config: Config,
) -> Service<PSProvider, ConsensusParamsProvider>
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
{
    Service::new(Task {
        shared_state: SharedState {
            // Maybe try to take the arc from the paramater
            consensus_parameters_provider: Arc::new(consensus_parameters_provider),
            current_height: Arc::new(RwLock::new(current_height)),
            pool: Arc::new(RwLock::new(Pool::new(
                ps_provider,
                // TODO: Real value
                GraphStorage::new(GraphConfig {
                    max_dependent_txn_count: 100
                }),
                BasicCollisionManager::new(),
                RatioTipGasSelection::new(),
                config,
            ))),
        },
    })
}
