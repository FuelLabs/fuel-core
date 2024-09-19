use std::sync::Arc;

use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    blockchain::consensus::Consensus,
    entities::relayer::transaction,
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    fuel_vm::checked_transaction::CheckedTransaction,
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipData,
            GossipsubMessageInfo,
            PeerId,
            TransactionGossipData,
        },
        txpool::PoolTransaction,
    },
};
use futures::StreamExt;
use parking_lot::RwLock;
use tokio::sync::Notify;

use crate::{
    collision_manager::basic::BasicCollisionManager,
    config::Config,
    error::Error,
    heavy_async_processing::HeavyAsyncProcessor,
    pool::Pool,
    ports::{
        AtomicView,
        BlockImporter as BlockImporterTrait,
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderTrait,
        MemoryPool as MemoryPoolTrait,
        TxPoolPersistentStorage,
        WasmChecker as WasmCheckerTrait,
        P2P as P2PTrait,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    shared_state::SharedState,
    storage::graph::{
        GraphConfig,
        GraphStorage,
    },
    verifications::perform_all_verifications,
};

pub type Service<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> = ServiceRunner<
    Task<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >,
>;

pub struct Task<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> {
    tx_from_p2p_stream: BoxStream<TransactionGossipData>,
    new_peers_subscribed_stream: BoxStream<PeerId>,
    imported_block_results_stream: BoxStream<SharedImportResult>,
    shared_state: SharedState<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >,
}

#[async_trait::async_trait]
impl<
        P2P,
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > RunnableService
    for Task<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    P2P: P2PTrait + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView> + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync + 'static,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync + 'static,
    WasmChecker: WasmCheckerTrait + Send + Sync + 'static,
    MemoryPool: MemoryPoolTrait + Send + Sync + 'static,
{
    const NAME: &'static str = "TxPoolv2";

    type SharedData = SharedState<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >;

    type Task = Task<
        P2P,
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
        P2P,
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > RunnableTask
    for Task<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    P2P: P2PTrait + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView> + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync + 'static,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync + 'static,
    WasmChecker: WasmCheckerTrait + Send + Sync + 'static,
    MemoryPool: MemoryPoolTrait + Send + Sync + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            _ = watcher.while_started() => {
                should_continue = false;
            }

            block_result = self.imported_block_results_stream.next() => {
                if let Some(result) = block_result {
                    self.manage_imported_block(result)?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            tx_from_p2p = self.tx_from_p2p_stream.next() => {
                if let Some(GossipData { data: Some(tx), message_id, peer_id }) = tx_from_p2p {
                    self.manage_tx_from_p2p(tx, message_id, peer_id)?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            new_peer_subscribed = self.new_peers_subscribed_stream.next() => {
                if let Some(peer_id) = new_peer_subscribed {
                    self.manage_new_peer_subscribed(peer_id);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }


        }
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<
        P2P,
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
    Task<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    P2P: P2PTrait + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView> + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync + 'static,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync + 'static,
    WasmChecker: WasmCheckerTrait + Send + Sync + 'static,
    MemoryPool: MemoryPoolTrait + Send + Sync + 'static,
{
    fn manage_imported_block(
        &mut self,
        result: SharedImportResult,
    ) -> anyhow::Result<()> {
        {
            let mut tx_pool = self.shared_state.pool.write();
            tx_pool
                .remove_committed_txs(result.tx_status.iter().map(|s| s.id).collect())
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        let new_height = *result.sealed_block.entity.header().height();
        {
            let mut block_height = self.shared_state.current_height.write();
            *block_height = new_height;
        }
        Ok(())
    }

    fn manage_tx_from_p2p(
        &mut self,
        tx: Transaction,
        message_id: Vec<u8>,
        peer_id: PeerId,
    ) -> anyhow::Result<()> {
        self.shared_state
            .insert(
                vec![tx],
                Some(GossipsubMessageInfo {
                    message_id,
                    peer_id,
                }),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    fn manage_new_peer_subscribed(&mut self, peer_id: PeerId) {
        self.shared_state.new_peer_subscribed(peer_id);
    }
}

pub fn new_service<
    P2P,
    BlockImporter,
    PSProvider,
    PSView,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
>(
    config: Config,
    p2p: P2P,
    block_importer: BlockImporter,
    ps_provider: PSProvider,
    consensus_parameters_provider: ConsensusParamsProvider,
    current_height: BlockHeight,
    gas_price_provider: GasPriceProvider,
    wasm_checker: WasmChecker,
    memory_pool: MemoryPool,
) -> Service<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
>
where
    P2P: P2PTrait<GossipedTransaction = TransactionGossipData> + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync,
    WasmChecker: WasmCheckerTrait + Send + Sync,
    MemoryPool: MemoryPoolTrait + Send + Sync,
    BlockImporter: BlockImporterTrait + Send + Sync,
    P2P: P2PTrait + Send + Sync,
{
    let tx_from_p2p_stream = p2p.gossiped_transaction_events();
    let new_peers_subscribed_stream = p2p.subscribe_new_peers();
    Service::new(Task {
        new_peers_subscribed_stream,
        tx_from_p2p_stream,
        imported_block_results_stream: block_importer.block_events(),
        shared_state: SharedState {
            p2p: Arc::new(p2p),
            consensus_parameters_provider: Arc::new(consensus_parameters_provider),
            gas_price_provider: Arc::new(gas_price_provider),
            wasm_checker: Arc::new(wasm_checker),
            memory: Arc::new(memory_pool),
            current_height: Arc::new(RwLock::new(current_height)),
            utxo_validation: config.utxo_validation,
            heavy_async_processor: Arc::new(
                HeavyAsyncProcessor::new(
                    config
                        .heavy_work
                        .number_pending_tasks_threads_verif_insert_transactions,
                    config.heavy_work.number_threads_verif_insert_transactions,
                )
                .unwrap(),
            ),
            pool: Arc::new(RwLock::new(Pool::new(
                ps_provider,
                GraphStorage::new(GraphConfig {
                    max_dependent_txn_count: config.max_dependent_txn_count,
                }),
                BasicCollisionManager::new(),
                RatioTipGasSelection::new(),
                config,
            ))),
            new_txs_notifier: Arc::new(Notify::new()),
        },
    })
}
