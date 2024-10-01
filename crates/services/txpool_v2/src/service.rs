use std::sync::Arc;

use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::txpool::PoolTransaction,
};
use parking_lot::RwLock;

use crate::{
    collision_manager::basic::BasicCollisionManager,
    config::Config,
    error::Error,
    heavy_async_processing::HeavyAsyncProcessor,
    pool::Pool,
    ports::{
        AtomicView,
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderTrait,
        MemoryPool as MemoryPoolTrait,
        TxPoolPersistentStorage,
        WasmChecker as WasmCheckerTrait,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    storage::{
        graph::{
            GraphConfig,
            GraphStorage,
        },
        Storage,
    },
    verifications::perform_all_verifications,
};

pub type RemovedTransactions = Vec<PoolTransaction>;
pub type InsertionResult = Result<RemovedTransactions, Error>;

pub type TxPool<PSProvider> = Arc<
    RwLock<
        Pool<
            PSProvider,
            GraphStorage,
            BasicCollisionManager<<GraphStorage as Storage>::StorageIndex>,
            RatioTipGasSelection<GraphStorage>,
        >,
    >,
>;

pub struct SharedState<
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> {
    pool: TxPool<PSProvider>,
    current_height: Arc<RwLock<BlockHeight>>,
    consensus_parameters_provider: Arc<ConsensusParamsProvider>,
    gas_price_provider: Arc<GasPriceProvider>,
    wasm_checker: Arc<WasmChecker>,
    memory: Arc<MemoryPool>,
    heavy_async_processor: Arc<HeavyAsyncProcessor>,
    utxo_validation: bool,
}

impl<PSProvider, ConsensusParamsProvider, GasPriceProvider, WasmChecker, MemoryPool> Clone
    for SharedState<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
{
    fn clone(&self) -> Self {
        SharedState {
            pool: self.pool.clone(),
            current_height: self.current_height.clone(),
            consensus_parameters_provider: self.consensus_parameters_provider.clone(),
            gas_price_provider: self.gas_price_provider.clone(),
            wasm_checker: self.wasm_checker.clone(),
            memory: self.memory.clone(),
            heavy_async_processor: self.heavy_async_processor.clone(),
            utxo_validation: self.utxo_validation,
        }
    }
}

impl<
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
    SharedState<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    PSProvider: AtomicView<LatestView = PSView> + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + 'static,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync + 'static,
    WasmChecker: WasmCheckerTrait + Send + Sync + 'static,
    MemoryPool: MemoryPoolTrait + Send + Sync + 'static,
{
    async fn insert(
        &self,
        transactions: Vec<Transaction>,
    ) -> Result<Vec<InsertionResult>, Error> {
        let current_height = *self.current_height.read();
        let (version, params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();
        let results = vec![];
        for transaction in transactions {
            self.heavy_async_processor.spawn({
                let shared_state = self.clone();
                let params = params.clone();
                async move {
                    // TODO: Return the error in the status update channel (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                    let checked_tx = perform_all_verifications(
                        transaction,
                        shared_state.pool.clone(),
                        current_height,
                        &params,
                        version,
                        shared_state.gas_price_provider.as_ref(),
                        shared_state.wasm_checker.as_ref(),
                        shared_state.memory.get_memory().await,
                    )
                    .await
                    .unwrap();
                    #[allow(unused_variables)]
                    let result = {
                        let mut pool = shared_state.pool.write();
                        // TODO: Return the result of the insertion (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                        pool.insert(checked_tx)
                    };
                }
            });
        }
        Ok(results)
    }
}

pub type Service<
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> = ServiceRunner<
    Task<PSProvider, ConsensusParamsProvider, GasPriceProvider, WasmChecker, MemoryPool>,
>;

pub struct Task<
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> {
    shared_state: SharedState<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >,
}

#[async_trait::async_trait]
impl<
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > RunnableService
    for Task<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync,
    WasmChecker: WasmCheckerTrait + Send + Sync,
    MemoryPool: MemoryPoolTrait + Send + Sync,
{
    const NAME: &'static str = "TxPoolv2";

    type SharedData = SharedState<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >;

    type Task = Task<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >;

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
impl<
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > RunnableTask
    for Task<
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync,
    WasmChecker: WasmCheckerTrait + Send + Sync,
    MemoryPool: MemoryPoolTrait + Send + Sync,
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

pub fn new_service<
    PSProvider,
    PSView,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
>(
    config: Config,
    ps_provider: PSProvider,
    consensus_parameters_provider: ConsensusParamsProvider,
    current_height: BlockHeight,
    gas_price_provider: GasPriceProvider,
    wasm_checker: WasmChecker,
    memory_pool: MemoryPool,
) -> Service<PSProvider, ConsensusParamsProvider, GasPriceProvider, WasmChecker, MemoryPool>
where
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync,
    WasmChecker: WasmCheckerTrait + Send + Sync,
    MemoryPool: MemoryPoolTrait + Send + Sync,
{
    Service::new(Task {
        shared_state: SharedState {
            consensus_parameters_provider: Arc::new(consensus_parameters_provider),
            gas_price_provider: Arc::new(gas_price_provider),
            wasm_checker: Arc::new(wasm_checker),
            memory: Arc::new(memory_pool),
            current_height: Arc::new(RwLock::new(current_height)),
            utxo_validation: config.utxo_validation,
            heavy_async_processor: Arc::new(
                HeavyAsyncProcessor::new(
                    config.heavy_work.number_threads_to_verify_transactions,
                    config.heavy_work.size_of_verification_queue,
                )
                .unwrap(),
            ),
            pool: Arc::new(RwLock::new(Pool::new(
                ps_provider,
                GraphStorage::new(GraphConfig {
                    max_txs_chain_count: config.max_txs_chain_count,
                }),
                BasicCollisionManager::new(),
                RatioTipGasSelection::new(),
                config,
            ))),
        },
    })
}
