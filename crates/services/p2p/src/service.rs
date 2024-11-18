use crate::{
    cached_view::CachedView,
    codecs::postcard::PostcardCodec,
    config::{
        Config,
        NotInitialized,
    },
    gossipsub::messages::{
        GossipTopicTag,
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    p2p_service::{
        FuelP2PEvent,
        FuelP2PService,
    },
    peer_manager::PeerInfo,
    ports::{
        BlockHeightImporter,
        P2pDb,
        TxPool,
    },
    request_response::messages::{
        OnResponse,
        RequestMessage,
        ResponseMessageErrorCode,
        ResponseSender,
        V2ResponseMessage,
    },
};
use anyhow::anyhow;
use fuel_core_metrics::p2p_metrics::set_blocks_requested;
use fuel_core_services::{
    stream::BoxStream,
    AsyncProcessor,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    SyncProcessor,
    TraceErr,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::SealedBlockHeader,
    fuel_tx::{
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    services::p2p::{
        peer_reputation::{
            AppScore,
            PeerReport,
        },
        BlockHeightHeartbeatData,
        GossipData,
        GossipsubMessageAcceptance,
        GossipsubMessageInfo,
        NetworkableTransactionPool,
        PeerId as FuelPeerId,
        TransactionGossipData,
        Transactions,
    },
};
use futures::{
    future::BoxFuture,
    StreamExt,
};
use libp2p::{
    gossipsub::{
        MessageAcceptance,
        PublishError,
    },
    request_response::InboundRequestId,
    PeerId,
};
use std::{
    fmt::Debug,
    future::Future,
    ops::Range,
    sync::Arc,
};
use tokio::{
    sync::{
        broadcast,
        mpsc::{
            self,
            Receiver,
        },
        oneshot,
    },
    time::{
        Duration,
        Instant,
    },
};
use tracing::warn;

const CHANNEL_SIZE: usize = 1024 * 10;

pub type Service<V, T> = ServiceRunner<UninitializedTask<V, SharedState, T>>;

pub enum TaskRequest {
    // Broadcast requests to p2p network
    BroadcastTransaction(Arc<Transaction>),
    // Request to get information about all connected peers
    GetAllPeerInfo {
        channel: oneshot::Sender<Vec<(PeerId, PeerInfo)>>,
    },
    GetSealedHeaders {
        block_height_range: Range<u32>,
        channel: OnResponse<Option<Vec<SealedBlockHeader>>>,
    },
    GetTransactions {
        block_height_range: Range<u32>,
        from_peer: PeerId,
        channel: OnResponse<Option<Vec<Transactions>>>,
    },
    TxPoolGetAllTxIds {
        from_peer: PeerId,
        channel: OnResponse<Option<Vec<TxId>>>,
    },
    TxPoolGetFullTransactions {
        tx_ids: Vec<TxId>,
        from_peer: PeerId,
        channel: OnResponse<Option<Vec<Option<NetworkableTransactionPool>>>>,
    },
    // Responds back to the p2p network
    RespondWithGossipsubMessageReport((GossipsubMessageInfo, GossipsubMessageAcceptance)),
    RespondWithPeerReport {
        peer_id: PeerId,
        score: AppScore,
        reporting_service: &'static str,
    },
    DatabaseTransactionsLookUp {
        response: Result<Vec<Transactions>, ResponseMessageErrorCode>,
        request_id: InboundRequestId,
    },
    DatabaseHeaderLookUp {
        response: Result<Vec<SealedBlockHeader>, ResponseMessageErrorCode>,
        request_id: InboundRequestId,
    },
    TxPoolAllTransactionsIds {
        response: Result<Vec<TxId>, ResponseMessageErrorCode>,
        request_id: InboundRequestId,
    },
    TxPoolFullTransactions {
        response:
            Result<Vec<Option<NetworkableTransactionPool>>, ResponseMessageErrorCode>,
        request_id: InboundRequestId,
    },
}

impl Debug for TaskRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskRequest::BroadcastTransaction(_) => {
                write!(f, "TaskRequest::BroadcastTransaction")
            }
            TaskRequest::GetSealedHeaders { .. } => {
                write!(f, "TaskRequest::GetSealedHeaders")
            }
            TaskRequest::GetTransactions { .. } => {
                write!(f, "TaskRequest::GetTransactions")
            }
            TaskRequest::TxPoolGetAllTxIds { .. } => {
                write!(f, "TaskRequest::TxPoolGetAllTxIds")
            }
            TaskRequest::TxPoolGetFullTransactions { .. } => {
                write!(f, "TaskRequest::TxPoolGetFullTransactions")
            }
            TaskRequest::RespondWithGossipsubMessageReport(_) => {
                write!(f, "TaskRequest::RespondWithGossipsubMessageReport")
            }
            TaskRequest::RespondWithPeerReport { .. } => {
                write!(f, "TaskRequest::RespondWithPeerReport")
            }
            TaskRequest::GetAllPeerInfo { .. } => {
                write!(f, "TaskRequest::GetPeerInfo")
            }
            TaskRequest::DatabaseTransactionsLookUp { .. } => {
                write!(f, "TaskRequest::DatabaseTransactionsLookUp")
            }
            TaskRequest::DatabaseHeaderLookUp { .. } => {
                write!(f, "TaskRequest::DatabaseHeaderLookUp")
            }
            TaskRequest::TxPoolAllTransactionsIds { .. } => {
                write!(f, "TaskRequest::TxPoolAllTransactionsIds")
            }
            TaskRequest::TxPoolFullTransactions { .. } => {
                write!(f, "TaskRequest::TxPoolFullTransactions")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartBeatPeerReportReason {
    OldHeartBeat,
    LowHeartBeatFrequency,
}

pub trait TaskP2PService: Send {
    fn get_all_peer_info(&self) -> Vec<(&PeerId, &PeerInfo)>;
    fn get_peer_id_with_height(&self, height: &BlockHeight) -> Option<PeerId>;

    fn next_event(&mut self) -> BoxFuture<'_, Option<FuelP2PEvent>>;

    fn publish_message(
        &mut self,
        message: GossipsubBroadcastRequest,
    ) -> anyhow::Result<()>;

    fn send_request_msg(
        &mut self,
        peer_id: Option<PeerId>,
        request_msg: RequestMessage,
        on_response: ResponseSender,
    ) -> anyhow::Result<()>;

    fn send_response_msg(
        &mut self,
        request_id: InboundRequestId,
        message: V2ResponseMessage,
    ) -> anyhow::Result<()>;

    fn report_message(
        &mut self,
        message: GossipsubMessageInfo,
        acceptance: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;

    fn report_peer(
        &mut self,
        peer_id: PeerId,
        score: AppScore,
        reporting_service: &str,
    ) -> anyhow::Result<()>;

    fn update_block_height(&mut self, height: BlockHeight) -> anyhow::Result<()>;

    fn update_metrics<T>(&self, update_fn: T)
    where
        T: FnOnce();
}

impl TaskP2PService for FuelP2PService {
    fn update_metrics<T>(&self, update_fn: T)
    where
        T: FnOnce(),
    {
        FuelP2PService::update_metrics(self, update_fn)
    }

    fn get_all_peer_info(&self) -> Vec<(&PeerId, &PeerInfo)> {
        self.peer_manager().get_all_peers().collect()
    }

    fn get_peer_id_with_height(&self, height: &BlockHeight) -> Option<PeerId> {
        self.peer_manager().get_peer_id_with_height(height)
    }

    fn next_event(&mut self) -> BoxFuture<'_, Option<FuelP2PEvent>> {
        Box::pin(self.next_event())
    }

    fn publish_message(
        &mut self,
        message: GossipsubBroadcastRequest,
    ) -> anyhow::Result<()> {
        let result = self.publish_message(message);

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                if matches!(&e, PublishError::InsufficientPeers) {
                    Ok(())
                } else {
                    Err(anyhow!(e))
                }
            }
        }
    }

    fn send_request_msg(
        &mut self,
        peer_id: Option<PeerId>,
        request_msg: RequestMessage,
        on_response: ResponseSender,
    ) -> anyhow::Result<()> {
        self.send_request_msg(peer_id, request_msg, on_response)?;
        Ok(())
    }

    fn send_response_msg(
        &mut self,
        request_id: InboundRequestId,
        message: V2ResponseMessage,
    ) -> anyhow::Result<()> {
        self.send_response_msg(request_id, message)?;
        Ok(())
    }

    fn report_message(
        &mut self,
        message: GossipsubMessageInfo,
        acceptance: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        report_message(self, message, acceptance);
        Ok(())
    }

    fn report_peer(
        &mut self,
        peer_id: PeerId,
        score: AppScore,
        reporting_service: &str,
    ) -> anyhow::Result<()> {
        self.report_peer(peer_id, score, reporting_service);
        Ok(())
    }

    fn update_block_height(&mut self, height: BlockHeight) -> anyhow::Result<()> {
        self.update_block_height(height);
        Ok(())
    }
}

pub trait Broadcast: Send {
    fn report_peer(
        &self,
        peer_id: FuelPeerId,
        report: AppScore,
        reporting_service: &'static str,
    ) -> anyhow::Result<()>;

    fn block_height_broadcast(
        &self,
        block_height_data: BlockHeightHeartbeatData,
    ) -> anyhow::Result<()>;

    fn tx_broadcast(&self, transaction: TransactionGossipData) -> anyhow::Result<()>;

    fn new_tx_subscription_broadcast(&self, peer_id: FuelPeerId) -> anyhow::Result<()>;
}

impl Broadcast for SharedState {
    fn report_peer(
        &self,
        peer_id: FuelPeerId,
        report: AppScore,
        reporting_service: &'static str,
    ) -> anyhow::Result<()> {
        self.report_peer(peer_id, report, reporting_service)
    }

    fn block_height_broadcast(
        &self,
        block_height_data: BlockHeightHeartbeatData,
    ) -> anyhow::Result<()> {
        self.block_height_broadcast.send(block_height_data)?;
        Ok(())
    }

    fn tx_broadcast(&self, transaction: TransactionGossipData) -> anyhow::Result<()> {
        self.tx_broadcast.send(transaction)?;
        Ok(())
    }

    fn new_tx_subscription_broadcast(&self, peer_id: FuelPeerId) -> anyhow::Result<()> {
        self.new_tx_subscription_broadcast.send(peer_id)?;
        Ok(())
    }
}

/// Uninitialized task for the p2p that can be upgraded later into [`Task`].
pub struct UninitializedTask<V, B, T> {
    chain_id: ChainId,
    view_provider: V,
    next_block_height: BoxStream<BlockHeight>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    broadcast: B,
    tx_pool: T,
    config: Config<NotInitialized>,
}

/// Orchestrates various p2p-related events between the inner `P2pService`
/// and the top level `NetworkService`.
pub struct Task<P, V, B, T> {
    chain_id: ChainId,
    response_timeout: Duration,
    p2p_service: P,
    view_provider: V,
    next_block_height: BoxStream<BlockHeight>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    request_sender: mpsc::Sender<TaskRequest>,
    db_heavy_task_processor: SyncProcessor,
    tx_pool_heavy_task_processor: AsyncProcessor,
    broadcast: B,
    tx_pool: T,
    max_headers_per_request: usize,
    max_txs_per_request: usize,
    // milliseconds wait time between peer heartbeat reputation checks
    heartbeat_check_interval: Duration,
    heartbeat_max_avg_interval: Duration,
    heartbeat_max_time_since_last: Duration,
    next_check_time: Instant,
    heartbeat_peer_reputation_config: HeartbeatPeerReputationConfig,
    // cached view
    cached_view: Arc<CachedView>,
}

#[derive(Default, Clone)]
pub struct HeartbeatPeerReputationConfig {
    old_heartbeat_penalty: AppScore,
    low_heartbeat_frequency_penalty: AppScore,
}

impl<V, T> UninitializedTask<V, SharedState, T> {
    pub fn new<B: BlockHeightImporter>(
        chain_id: ChainId,
        config: Config<NotInitialized>,
        shared_state: SharedState,
        request_receiver: Receiver<TaskRequest>,
        view_provider: V,
        block_importer: B,
        tx_pool: T,
    ) -> Self {
        let next_block_height = block_importer.next_block_height();

        Self {
            chain_id,
            view_provider,
            tx_pool,
            next_block_height,
            request_receiver,
            broadcast: shared_state,
            config,
        }
    }
}

impl<P, V, B, T> Task<P, V, B, T>
where
    P: TaskP2PService,
    V: AtomicView,
    B: Broadcast,
{
    fn peer_heartbeat_reputation_checks(&self) -> anyhow::Result<()> {
        for (peer_id, peer_info) in self.p2p_service.get_all_peer_info() {
            if peer_info.heartbeat_data.duration_since_last_heartbeat()
                > self.heartbeat_max_time_since_last
            {
                tracing::debug!("Peer {:?} has old heartbeat", peer_id);
                let report = HeartBeatPeerReportReason::OldHeartBeat;
                let peer_id = convert_peer_id(peer_id)?;
                self.report_peer(peer_id, report)?;
            } else if peer_info.heartbeat_data.average_time_between_heartbeats()
                > self.heartbeat_max_avg_interval
            {
                tracing::debug!("Peer {:?} has low heartbeat frequency", peer_id);
                let report = HeartBeatPeerReportReason::LowHeartBeatFrequency;
                let peer_id = convert_peer_id(peer_id)?;
                self.report_peer(peer_id, report)?;
            }
        }
        Ok(())
    }

    fn report_peer(
        &self,
        peer_id: FuelPeerId,
        report: HeartBeatPeerReportReason,
    ) -> anyhow::Result<()> {
        let app_score = match report {
            HeartBeatPeerReportReason::OldHeartBeat => {
                self.heartbeat_peer_reputation_config.old_heartbeat_penalty
            }
            HeartBeatPeerReportReason::LowHeartBeatFrequency => {
                self.heartbeat_peer_reputation_config
                    .low_heartbeat_frequency_penalty
            }
        };
        let reporting_service = "p2p";
        self.broadcast
            .report_peer(peer_id, app_score, reporting_service)?;
        Ok(())
    }
}

impl<P, V, B, T> Task<P, V, B, T>
where
    P: TaskP2PService + 'static,
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    T: TxPool + 'static,
    B: Send,
{
    fn update_metrics<U>(&self, update_fn: U)
    where
        U: FnOnce(),
    {
        self.p2p_service.update_metrics(update_fn)
    }

    fn process_request(
        &mut self,
        request_message: RequestMessage,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        match request_message {
            RequestMessage::Transactions(range) => {
                self.handle_transactions_request(range, request_id)
            }
            RequestMessage::SealedHeaders(range) => {
                self.handle_sealed_headers_request(range, request_id)
            }
            RequestMessage::TxPoolAllTransactionsIds => {
                self.handle_all_transactions_ids_request(request_id)
            }
            RequestMessage::TxPoolFullTransactions(tx_ids) => {
                self.handle_full_transactions_request(tx_ids, request_id)
            }
        }
    }

    fn handle_db_request<DbLookUpFn, ResponseSenderFn, TaskRequestFn, R>(
        &mut self,
        range: Range<u32>,
        request_id: InboundRequestId,
        response_sender: ResponseSenderFn,
        db_lookup: DbLookUpFn,
        task_request: TaskRequestFn,
        max_len: usize,
    ) -> anyhow::Result<()>
    where
        DbLookUpFn: Fn(&V::LatestView, &Arc<CachedView>, Range<u32>) -> anyhow::Result<Option<R>>
            + Send
            + 'static,
        ResponseSenderFn:
            Fn(Result<R, ResponseMessageErrorCode>) -> V2ResponseMessage + Send + 'static,
        TaskRequestFn: Fn(Result<R, ResponseMessageErrorCode>, InboundRequestId) -> TaskRequest
            + Send
            + 'static,
        R: Send + 'static,
    {
        let instant = Instant::now();
        let timeout = self.response_timeout;
        let response_channel = self.request_sender.clone();
        let range_len = range.len();

        self.update_metrics(|| set_blocks_requested(range_len));

        if range_len > max_len {
            tracing::error!(
                requested_length = range.len(),
                max_len,
                "Requested range is too big"
            );
            // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
            // Return helpful error message to requester.
            let response = Err(ResponseMessageErrorCode::ProtocolV1EmptyResponse);
            let _ = self
                .p2p_service
                .send_response_msg(request_id, response_sender(response));
            return Ok(());
        }

        let view = self.view_provider.latest_view()?;
        let result = self.db_heavy_task_processor.try_spawn({
            let cached_view = self.cached_view.clone();
            move || {
                if instant.elapsed() > timeout {
                    tracing::warn!("Request timed out");
                    return;
                }

                // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
                // Add new error code
                let response = db_lookup(&view, &cached_view, range.clone())
                    .ok()
                    .flatten()
                    .ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse);

                let _ = response_channel
                    .try_send(task_request(response, request_id))
                    .trace_err("Failed to send response to the request channel");
            }
        });

        // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
        // Handle error cases and return meaningful status codes
        if result.is_err() {
            let err = Err(ResponseMessageErrorCode::ProtocolV1EmptyResponse);
            let _ = self
                .p2p_service
                .send_response_msg(request_id, response_sender(err));
        }

        Ok(())
    }

    fn handle_transactions_request(
        &mut self,
        range: Range<u32>,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        self.handle_db_request(
            range,
            request_id,
            V2ResponseMessage::Transactions,
            |view, cached_view, range| {
                cached_view
                    .get_transactions(view, range)
                    .map_err(anyhow::Error::from)
            },
            |response, request_id| TaskRequest::DatabaseTransactionsLookUp {
                response,
                request_id,
            },
            self.max_headers_per_request,
        )
    }

    fn handle_sealed_headers_request(
        &mut self,
        range: Range<u32>,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        self.handle_db_request(
            range,
            request_id,
            V2ResponseMessage::SealedHeaders,
            |view, cached_view, range| {
                cached_view
                    .get_sealed_headers(view, range)
                    .map_err(anyhow::Error::from)
            },
            |response, request_id| TaskRequest::DatabaseHeaderLookUp {
                response,
                request_id,
            },
            self.max_headers_per_request,
        )
    }

    fn handle_txpool_request<F, ResponseSenderFn, TaskRequestFn, R>(
        &mut self,
        request_id: InboundRequestId,
        txpool_function: F,
        response_sender: ResponseSenderFn,
        task_request: TaskRequestFn,
    ) -> anyhow::Result<()>
    where
        ResponseSenderFn:
            Fn(Result<R, ResponseMessageErrorCode>) -> V2ResponseMessage + Send + 'static,
        TaskRequestFn: Fn(Result<R, ResponseMessageErrorCode>, InboundRequestId) -> TaskRequest
            + Send
            + 'static,
        F: Future<Output = anyhow::Result<R>> + Send + 'static,
    {
        let instant = Instant::now();
        let timeout = self.response_timeout;
        let response_channel = self.request_sender.clone();
        let result = self.tx_pool_heavy_task_processor.try_spawn(async move {
            if instant.elapsed() > timeout {
                tracing::warn!("Request timed out");
                return;
            }

            let Ok(response) = txpool_function.await else {
                warn!("Failed to get txpool data");
                return;
            };

            // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
            // Return helpful error message to requester.
            let _ = response_channel
                .try_send(task_request(Ok(response), request_id))
                .trace_err("Failed to send response to the request channel");
        });

        if result.is_err() {
            // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
            // Return better error code
            let res = Err(ResponseMessageErrorCode::ProtocolV1EmptyResponse);
            let _ = self
                .p2p_service
                .send_response_msg(request_id, response_sender(res));
        }

        Ok(())
    }

    fn handle_all_transactions_ids_request(
        &mut self,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        let max_txs = self.max_txs_per_request;
        let tx_pool = self.tx_pool.clone();
        self.handle_txpool_request(
            request_id,
            async move { tx_pool.get_tx_ids(max_txs).await },
            V2ResponseMessage::TxPoolAllTransactionsIds,
            |response, request_id| TaskRequest::TxPoolAllTransactionsIds {
                response,
                request_id,
            },
        )
    }

    fn handle_full_transactions_request(
        &mut self,
        tx_ids: Vec<TxId>,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
        // Return helpful error message to requester.
        if tx_ids.len() > self.max_txs_per_request {
            self.p2p_service.send_response_msg(
                request_id,
                V2ResponseMessage::TxPoolFullTransactions(Err(
                    ResponseMessageErrorCode::ProtocolV1EmptyResponse,
                )),
            )?;
            return Ok(());
        }
        let tx_pool = self.tx_pool.clone();
        self.handle_txpool_request(
            request_id,
            async move { tx_pool.get_full_txs(tx_ids).await },
            V2ResponseMessage::TxPoolFullTransactions,
            |response, request_id| TaskRequest::TxPoolFullTransactions {
                response,
                request_id,
            },
        )
    }
}

fn convert_peer_id(peer_id: &PeerId) -> anyhow::Result<FuelPeerId> {
    let inner = Vec::from(*peer_id);
    Ok(FuelPeerId::from(inner))
}

#[async_trait::async_trait]
impl<V, T> RunnableService for UninitializedTask<V, SharedState, T>
where
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    T: TxPool + 'static,
{
    const NAME: &'static str = "P2P";

    type SharedData = SharedState;
    type Task = Task<FuelP2PService, V, SharedState, T>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.broadcast.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let Self {
            chain_id,
            view_provider,
            next_block_height,
            request_receiver,
            broadcast,
            tx_pool,
            config,
        } = self;

        let view = view_provider.latest_view()?;
        let genesis = view.get_genesis()?;
        let config = config.init(genesis)?;
        let Config {
            max_block_size,
            max_headers_per_request,
            max_txs_per_request,
            heartbeat_check_interval,
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            database_read_threads,
            tx_pool_threads,
            metrics,
            ..
        } = config;

        // Hardcoded for now, but left here to be configurable in the future.
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1340
        let heartbeat_peer_reputation_config = HeartbeatPeerReputationConfig {
            old_heartbeat_penalty: -5.,
            low_heartbeat_frequency_penalty: -5.,
        };

        let response_timeout = config.set_request_timeout;
        let mut p2p_service = FuelP2PService::new(
            broadcast.reserved_peers_broadcast.clone(),
            config,
            PostcardCodec::new(max_block_size),
        )
        .await?;
        p2p_service.start().await?;

        let next_check_time =
            Instant::now().checked_add(heartbeat_check_interval).expect(
                "The heartbeat check interval should be small enough to do frequently",
            );
        let db_heavy_task_processor = SyncProcessor::new(
            "P2P_DatabaseProcessor",
            database_read_threads,
            1024 * 10,
        )?;
        let tx_pool_heavy_task_processor =
            AsyncProcessor::new("P2P_TxPoolLookUpProcessor", tx_pool_threads, 32)?;
        let request_sender = broadcast.request_sender.clone();

        let task = Task {
            chain_id,
            response_timeout,
            p2p_service,
            view_provider,
            request_receiver,
            request_sender,
            next_block_height,
            broadcast,
            tx_pool,
            db_heavy_task_processor,
            tx_pool_heavy_task_processor,
            max_headers_per_request,
            max_txs_per_request,
            heartbeat_check_interval,
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time,
            heartbeat_peer_reputation_config,
            cached_view: Arc::new(CachedView::new(614 * 10, metrics)),
        };
        Ok(task)
    }
}

// TODO: Add tests https://github.com/FuelLabs/fuel-core/issues/1275
#[async_trait::async_trait]
impl<P, V, B, T> RunnableTask for Task<P, V, B, T>
where
    P: TaskP2PService + 'static,
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    B: Broadcast + 'static,
    T: TxPool + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        tracing::debug!("P2P task is running");
        let mut should_continue;

        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            },
            latest_block_height = self.next_block_height.next() => {
                if let Some(latest_block_height) = latest_block_height {
                    let _ = self.p2p_service.update_block_height(latest_block_height);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            },
            next_service_request = self.request_receiver.recv() => {
                should_continue = true;
                match next_service_request {
                    Some(TaskRequest::BroadcastTransaction(transaction)) => {
                        let tx_id = transaction.id(&self.chain_id);
                        let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                        let result = self.p2p_service.publish_message(broadcast);
                        if let Err(e) = result {
                            tracing::error!("Got an error during transaction {} broadcasting {}", tx_id, e);
                        }
                    }
                    Some(TaskRequest::GetSealedHeaders { block_height_range, channel}) => {
                        let channel = ResponseSender::SealedHeaders(channel);
                        let request_msg = RequestMessage::SealedHeaders(block_height_range.clone());

                        // Note: this range has already been checked for
                        // validity in `SharedState::get_sealed_block_headers`.
                        let height = BlockHeight::from(block_height_range.end.saturating_sub(1));
                        let peer = self.p2p_service.get_peer_id_with_height(&height);
                        if self.p2p_service.send_request_msg(peer, request_msg, channel).is_err() {
                            tracing::warn!("No peers found for block at height {:?}", height);
                        }
                    }
                    Some(TaskRequest::GetTransactions { block_height_range, from_peer, channel }) => {
                        let channel = ResponseSender::Transactions(channel);
                        let request_msg = RequestMessage::Transactions(block_height_range);
                        self.p2p_service.send_request_msg(Some(from_peer), request_msg, channel).expect("We always a peer here, so send has a target");
                    }
                    Some(TaskRequest::TxPoolGetAllTxIds { from_peer, channel }) => {
                        let channel = ResponseSender::TxPoolAllTransactionsIds(channel);
                        let request_msg = RequestMessage::TxPoolAllTransactionsIds;
                        self.p2p_service.send_request_msg(Some(from_peer), request_msg, channel).expect("We always have a peer here, so send has a target");
                    }
                    Some(TaskRequest::TxPoolGetFullTransactions { tx_ids, from_peer, channel }) => {
                        let channel = ResponseSender::TxPoolFullTransactions(channel);
                        let request_msg = RequestMessage::TxPoolFullTransactions(tx_ids);
                        self.p2p_service.send_request_msg(Some(from_peer), request_msg, channel).expect("We always have a peer here, so send has a target");
                    }
                    Some(TaskRequest::RespondWithGossipsubMessageReport((message, acceptance))) => {
                        // report_message(&mut self.p2p_service, message, acceptance);
                        self.p2p_service.report_message(message, acceptance)?;
                    }
                    Some(TaskRequest::RespondWithPeerReport { peer_id, score, reporting_service }) => {
                        let _ = self.p2p_service.report_peer(peer_id, score, reporting_service);
                    }
                    Some(TaskRequest::GetAllPeerInfo { channel }) => {
                        let peers = self.p2p_service.get_all_peer_info()
                            .into_iter()
                            .map(|(id, info)| (*id, info.clone()))
                            .collect::<Vec<_>>();
                        let _ = channel.send(peers);
                    }
                    Some(TaskRequest::DatabaseTransactionsLookUp { response, request_id }) => {
                        let _ = self.p2p_service.send_response_msg(request_id, V2ResponseMessage::Transactions(response));
                    }
                    Some(TaskRequest::DatabaseHeaderLookUp { response, request_id }) => {
                        let _ = self.p2p_service.send_response_msg(request_id, V2ResponseMessage::SealedHeaders(response));
                    }
                    Some(TaskRequest::TxPoolAllTransactionsIds { response, request_id }) => {
                        let _ = self.p2p_service.send_response_msg(request_id, V2ResponseMessage::TxPoolAllTransactionsIds(response));
                    }
                    Some(TaskRequest::TxPoolFullTransactions { response, request_id }) => {
                        let _ = self.p2p_service.send_response_msg(request_id, V2ResponseMessage::TxPoolFullTransactions(response));
                    }
                    None => {
                        tracing::error!("The P2P `Task` should be holder of the `Sender`");
                        should_continue = false;
                    }
                }
            }
            p2p_event = self.p2p_service.next_event() => {
                should_continue = true;
                match p2p_event {
                    Some(FuelP2PEvent::PeerInfoUpdated { peer_id, block_height }) => {
                        let peer_id: Vec<u8> = peer_id.into();
                        let block_height_data = BlockHeightHeartbeatData {
                            peer_id: peer_id.into(),
                            block_height,
                        };

                        let _ = self.broadcast.block_height_broadcast(block_height_data);
                    }
                    Some(FuelP2PEvent::GossipsubMessage { message, message_id, peer_id,.. }) => {
                        let message_id = message_id.0;

                        match message {
                            GossipsubMessage::NewTx(transaction) => {
                                let next_transaction = GossipData::new(transaction, peer_id, message_id);
                                let _ = self.broadcast.tx_broadcast(next_transaction);
                            },
                        }
                    },
                    Some(FuelP2PEvent::InboundRequestMessage { request_message, request_id }) => {
                        self.process_request(request_message, request_id)?
                    },
                    Some(FuelP2PEvent::NewSubscription { peer_id, tag }) => {
                        if tag == GossipTopicTag::NewTx {
                            let _ = self.broadcast.new_tx_subscription_broadcast(FuelPeerId::from(peer_id.to_bytes()));
                        }
                    },
                    _ => (),
                }
            },
            _  = tokio::time::sleep_until(self.next_check_time) => {
                should_continue = true;
                let res = self.peer_heartbeat_reputation_checks();
                match res {
                    Ok(_) => tracing::debug!("Peer heartbeat reputation checks completed"),
                    Err(e) => {
                        tracing::error!("Failed to perform peer heartbeat reputation checks: {:?}", e);
                    }
                }
                self.next_check_time += self.heartbeat_check_interval;
            }
        }

        tracing::debug!("P2P task is finished");
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.

        // `FuelP2PService` doesn't support graceful shutdown(with informing of connected peers).
        // https://github.com/libp2p/specs/blob/master/ROADMAP.md#%EF%B8%8F-polite-peering
        // Dropping of the `FuelP2PService` will close all connections.

        Ok(())
    }
}

#[derive(Clone)]
pub struct SharedState {
    /// Sender of p2p with peer gossip subscription (vec<u8> represent the peer_id)
    new_tx_subscription_broadcast: broadcast::Sender<FuelPeerId>,
    /// Sender of p2p transaction used for subscribing.
    tx_broadcast: broadcast::Sender<TransactionGossipData>,
    /// Sender of reserved peers connection updates.
    reserved_peers_broadcast: broadcast::Sender<usize>,
    /// Used for communicating with the `Task`.
    request_sender: mpsc::Sender<TaskRequest>,
    /// Sender of p2p block height data
    block_height_broadcast: broadcast::Sender<BlockHeightHeartbeatData>,
    /// Max txs per request
    max_txs_per_request: usize,
}

impl SharedState {
    pub fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        acceptance: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        self.request_sender
            .try_send(TaskRequest::RespondWithGossipsubMessageReport((
                message_info,
                acceptance,
            )))?;
        Ok(())
    }

    pub async fn get_sealed_block_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> anyhow::Result<(Vec<u8>, Option<Vec<SealedBlockHeader>>)> {
        let (sender, receiver) = oneshot::channel();

        if block_height_range.is_empty() {
            return Err(anyhow!(
                "Cannot retrieve headers for an empty range of block heights"
            ));
        }

        self.request_sender
            .send(TaskRequest::GetSealedHeaders {
                block_height_range,
                channel: sender,
            })
            .await?;

        let (peer_id, response) = receiver.await.map_err(|e| anyhow!("{e}"))?;

        let data = response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))?;
        Ok((peer_id.to_bytes(), data))
    }

    pub async fn get_transactions_from_peer(
        &self,
        peer_id: FuelPeerId,
        range: Range<u32>,
    ) -> anyhow::Result<Option<Vec<Transactions>>> {
        let (sender, receiver) = oneshot::channel();
        let from_peer = PeerId::from_bytes(peer_id.as_ref()).expect("Valid PeerId");

        let request = TaskRequest::GetTransactions {
            block_height_range: range,
            from_peer,
            channel: sender,
        };
        self.request_sender.send(request).await?;

        let (response_from_peer, response) =
            receiver.await.map_err(|e| anyhow!("{e}"))?;
        assert_eq!(
            peer_id.as_ref(),
            response_from_peer.to_bytes(),
            "Bug: response from non-requested peer"
        );

        response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))
    }

    pub async fn get_all_transactions_ids_from_peer(
        &self,
        peer_id: FuelPeerId,
    ) -> anyhow::Result<Vec<TxId>> {
        let (sender, receiver) = oneshot::channel();
        let from_peer = PeerId::from_bytes(peer_id.as_ref()).expect("Valid PeerId");
        let request = TaskRequest::TxPoolGetAllTxIds {
            from_peer,
            channel: sender,
        };
        self.request_sender.try_send(request)?;

        let (response_from_peer, response) =
            receiver.await.map_err(|e| anyhow!("{e}"))?;

        debug_assert_eq!(
            peer_id.as_ref(),
            response_from_peer.to_bytes(),
            "Bug: response from non-requested peer"
        );

        let Some(txs) =
            response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))?
        else {
            return Ok(vec![]);
        };
        if txs.len() > self.max_txs_per_request {
            return Err(anyhow!("Too many transactions requested: {}", txs.len()));
        }
        Ok(txs)
    }

    pub async fn get_full_transactions_from_peer(
        &self,
        peer_id: FuelPeerId,
        tx_ids: Vec<TxId>,
    ) -> anyhow::Result<Vec<Option<Transaction>>> {
        let (sender, receiver) = oneshot::channel();
        let from_peer = PeerId::from_bytes(peer_id.as_ref()).expect("Valid PeerId");
        let request = TaskRequest::TxPoolGetFullTransactions {
            tx_ids,
            from_peer,
            channel: sender,
        };
        self.request_sender.try_send(request)?;

        let (response_from_peer, response) =
            receiver.await.map_err(|e| anyhow!("{e}"))?;
        debug_assert_eq!(
            peer_id.as_ref(),
            response_from_peer.to_bytes(),
            "Bug: response from non-requested peer"
        );

        let Some(txs) =
            response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))?
        else {
            return Ok(vec![]);
        };
        if txs.len() > self.max_txs_per_request {
            return Err(anyhow!("Too many transactions requested: {}", txs.len()));
        }
        txs.into_iter()
            .map(|tx| {
                tx.map(Transaction::try_from)
                    .transpose()
                    .map_err(|err| anyhow::anyhow!(err))
            })
            .collect()
    }

    pub fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        self.request_sender
            .try_send(TaskRequest::BroadcastTransaction(transaction))?;
        Ok(())
    }

    pub async fn get_all_peers(&self) -> anyhow::Result<Vec<(PeerId, PeerInfo)>> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(TaskRequest::GetAllPeerInfo { channel: sender })
            .await?;

        receiver.await.map_err(|e| anyhow!("{}", e))
    }

    pub fn subscribe_new_peers(&self) -> broadcast::Receiver<FuelPeerId> {
        self.new_tx_subscription_broadcast.subscribe()
    }

    pub fn subscribe_tx(&self) -> broadcast::Receiver<TransactionGossipData> {
        self.tx_broadcast.subscribe()
    }

    pub fn subscribe_block_height(
        &self,
    ) -> broadcast::Receiver<BlockHeightHeartbeatData> {
        self.block_height_broadcast.subscribe()
    }

    pub fn subscribe_reserved_peers_count(&self) -> broadcast::Receiver<usize> {
        self.reserved_peers_broadcast.subscribe()
    }

    pub fn report_peer<T: PeerReport>(
        &self,
        peer_id: FuelPeerId,
        peer_report: T,
        reporting_service: &'static str,
    ) -> anyhow::Result<()> {
        match Vec::from(peer_id).try_into() {
            Ok(peer_id) => {
                let score = peer_report.get_score_from_report();

                self.request_sender
                    .try_send(TaskRequest::RespondWithPeerReport {
                        peer_id,
                        score,
                        reporting_service,
                    })?;

                Ok(())
            }
            Err(e) => {
                warn!(target: "fuel-p2p", "Failed to read PeerId from {e:?}");
                Err(anyhow::anyhow!("Failed to read PeerId from {e:?}"))
            }
        }
    }
}

pub fn build_shared_state(
    config: Config<NotInitialized>,
) -> (SharedState, Receiver<TaskRequest>) {
    let (request_sender, request_receiver) = mpsc::channel(CHANNEL_SIZE);
    let (tx_broadcast, _) = broadcast::channel(CHANNEL_SIZE);
    let (new_tx_subscription_broadcast, _) = broadcast::channel(CHANNEL_SIZE);
    let (block_height_broadcast, _) = broadcast::channel(CHANNEL_SIZE);

    let (reserved_peers_broadcast, _) = broadcast::channel::<usize>(
        config
            .reserved_nodes
            .len()
            .saturating_mul(2)
            .saturating_add(1),
    );

    (
        SharedState {
            request_sender,
            new_tx_subscription_broadcast,
            tx_broadcast,
            reserved_peers_broadcast,
            block_height_broadcast,
            max_txs_per_request: config.max_txs_per_request,
        },
        request_receiver,
    )
}

pub fn new_service<V, B, T>(
    chain_id: ChainId,
    p2p_config: Config<NotInitialized>,
    shared_state: SharedState,
    request_receiver: Receiver<TaskRequest>,
    view_provider: V,
    block_importer: B,
    tx_pool: T,
) -> Service<V, T>
where
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    B: BlockHeightImporter,
    T: TxPool,
{
    let task = UninitializedTask::new(
        chain_id,
        p2p_config,
        shared_state,
        request_receiver,
        view_provider,
        block_importer,
        tx_pool,
    );
    Service::new(task)
}

pub fn to_message_acceptance(
    acceptance: &GossipsubMessageAcceptance,
) -> MessageAcceptance {
    match acceptance {
        GossipsubMessageAcceptance::Accept => MessageAcceptance::Accept,
        GossipsubMessageAcceptance::Reject => MessageAcceptance::Reject,
        GossipsubMessageAcceptance::Ignore => MessageAcceptance::Ignore,
    }
}

fn report_message(
    p2p_service: &mut FuelP2PService,
    message: GossipsubMessageInfo,
    acceptance: GossipsubMessageAcceptance,
) {
    let GossipsubMessageInfo {
        peer_id,
        message_id,
    } = message;

    let msg_id = message_id.into();
    let peer_id: Vec<u8> = peer_id.into();

    if let Ok(peer_id) = peer_id.try_into() {
        let acceptance = to_message_acceptance(&acceptance);
        p2p_service.report_message_validation_result(&msg_id, peer_id, acceptance);
    } else {
        warn!(target: "fuel-p2p", "Failed to read PeerId from received GossipsubMessageId: {}", msg_id);
    }
}

#[cfg(test)]
pub mod tests {
    #![allow(non_snake_case)]
    use crate::ports::P2pDb;

    use super::*;

    use crate::peer_manager::heartbeat_data::HeartbeatData;
    use fuel_core_services::{
        Service,
        State,
    };
    use fuel_core_storage::Result as StorageResult;
    use fuel_core_types::{
        blockchain::consensus::Genesis,
        fuel_types::BlockHeight,
    };
    use futures::FutureExt;
    use std::{
        collections::VecDeque,
        time::SystemTime,
    };

    #[derive(Clone, Debug)]
    struct FakeDb;

    impl AtomicView for FakeDb {
        type LatestView = Self;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(self.clone())
        }
    }

    impl P2pDb for FakeDb {
        fn get_sealed_headers(
            &self,
            _block_height_range: Range<u32>,
        ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
            unimplemented!()
        }

        fn get_transactions(
            &self,
            _block_height_range: Range<u32>,
        ) -> StorageResult<Option<Vec<Transactions>>> {
            unimplemented!()
        }

        fn get_genesis(&self) -> StorageResult<Genesis> {
            Ok(Default::default())
        }
    }

    #[derive(Clone, Debug)]
    struct FakeBlockImporter;

    impl BlockHeightImporter for FakeBlockImporter {
        fn next_block_height(&self) -> BoxStream<BlockHeight> {
            Box::pin(fuel_core_services::stream::pending())
        }
    }

    #[derive(Clone, Debug)]
    struct FakeTxPool;

    impl TxPool for FakeTxPool {
        async fn get_tx_ids(
            &self,
            _max_txs: usize,
        ) -> anyhow::Result<Vec<fuel_core_types::fuel_tx::TxId>> {
            Ok(vec![])
        }

        async fn get_full_txs(
            &self,
            tx_ids: Vec<TxId>,
        ) -> anyhow::Result<Vec<Option<NetworkableTransactionPool>>> {
            Ok(tx_ids.iter().map(|_| None).collect())
        }
    }

    #[tokio::test]
    async fn start_and_stop_awaits_works() {
        let p2p_config = Config::<NotInitialized>::default("start_stop_works");
        let (shared_state, request_receiver) = build_shared_state(p2p_config.clone());
        let service = new_service(
            ChainId::default(),
            p2p_config,
            shared_state,
            request_receiver,
            FakeDb,
            FakeBlockImporter,
            FakeTxPool,
        );

        // Node with p2p service started
        assert!(service.start_and_await().await.unwrap().started());
        // Node with p2p service stopped
        assert!(service.stop_and_await().await.unwrap().stopped());
    }

    struct FakeP2PService {
        peer_info: Vec<(PeerId, PeerInfo)>,
        next_event_stream: BoxStream<FuelP2PEvent>,
    }

    impl TaskP2PService for FakeP2PService {
        fn update_metrics<T>(&self, _: T)
        where
            T: FnOnce(),
        {
            unimplemented!()
        }

        fn get_all_peer_info(&self) -> Vec<(&PeerId, &PeerInfo)> {
            self.peer_info.iter().map(|tup| (&tup.0, &tup.1)).collect()
        }

        fn get_peer_id_with_height(&self, _height: &BlockHeight) -> Option<PeerId> {
            todo!()
        }

        fn next_event(&mut self) -> BoxFuture<'_, Option<FuelP2PEvent>> {
            self.next_event_stream.next().boxed()
        }

        fn publish_message(
            &mut self,
            _message: GossipsubBroadcastRequest,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn send_request_msg(
            &mut self,
            _peer_id: Option<PeerId>,
            _request_msg: RequestMessage,
            _on_response: ResponseSender,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn send_response_msg(
            &mut self,
            _request_id: InboundRequestId,
            _message: V2ResponseMessage,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn report_message(
            &mut self,
            _message: GossipsubMessageInfo,
            _acceptance: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn report_peer(
            &mut self,
            _peer_id: PeerId,
            _score: AppScore,
            _reporting_service: &str,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn update_block_height(&mut self, _height: BlockHeight) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeDB;

    impl AtomicView for FakeDB {
        type LatestView = Self;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(self.clone())
        }
    }

    impl P2pDb for FakeDB {
        fn get_sealed_headers(
            &self,
            _block_height_range: Range<u32>,
        ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
            todo!()
        }

        fn get_transactions(
            &self,
            _block_height_range: Range<u32>,
        ) -> StorageResult<Option<Vec<Transactions>>> {
            todo!()
        }

        fn get_genesis(&self) -> StorageResult<Genesis> {
            todo!()
        }
    }

    struct FakeBroadcast {
        pub peer_reports: mpsc::Sender<(FuelPeerId, AppScore, String)>,
    }

    impl Broadcast for FakeBroadcast {
        fn report_peer(
            &self,
            peer_id: FuelPeerId,
            report: AppScore,
            reporting_service: &'static str,
        ) -> anyhow::Result<()> {
            self.peer_reports.try_send((
                peer_id,
                report,
                reporting_service.to_string(),
            ))?;
            Ok(())
        }

        fn block_height_broadcast(
            &self,
            _block_height_data: BlockHeightHeartbeatData,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn tx_broadcast(
            &self,
            _transaction: TransactionGossipData,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn new_tx_subscription_broadcast(
            &self,
            _peer_id: FuelPeerId,
        ) -> anyhow::Result<()> {
            todo!()
        }
    }

    #[tokio::test]
    async fn peer_heartbeat_reputation_checks__slow_heartbeat_sends_reports() {
        // given
        let peer_id = PeerId::random();
        // more than limit
        let last_duration = Duration::from_secs(30);
        let mut durations = VecDeque::new();
        durations.push_front(last_duration);

        let heartbeat_data = HeartbeatData {
            block_height: None,
            last_heartbeat: Instant::now(),
            last_heartbeat_sys: SystemTime::now(),
            window: 0,
            durations,
        };
        let peer_info = PeerInfo {
            peer_addresses: Default::default(),
            client_version: None,
            heartbeat_data,
            score: 100.0,
        };
        let peer_info = vec![(peer_id, peer_info)];
        let p2p_service = FakeP2PService {
            peer_info,
            next_event_stream: Box::pin(futures::stream::pending()),
        };
        let (request_sender, request_receiver) = mpsc::channel(100);

        let (report_sender, mut report_receiver) = mpsc::channel(100);
        let broadcast = FakeBroadcast {
            peer_reports: report_sender,
        };

        // Less than actual
        let heartbeat_max_avg_interval = Duration::from_secs(20);
        // Greater than actual
        let heartbeat_max_time_since_last = Duration::from_secs(40);

        // Arbitrary values
        let heartbeat_peer_reputation_config = HeartbeatPeerReputationConfig {
            old_heartbeat_penalty: 5.6,
            low_heartbeat_frequency_penalty: 20.45,
        };

        let mut task = Task {
            chain_id: Default::default(),
            response_timeout: Default::default(),
            p2p_service,
            view_provider: FakeDB,
            next_block_height: FakeBlockImporter.next_block_height(),
            tx_pool: FakeTxPool,
            request_receiver,
            request_sender,
            db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
            tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
            broadcast,
            max_headers_per_request: 0,
            max_txs_per_request: 100,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time: Instant::now(),
            heartbeat_peer_reputation_config: heartbeat_peer_reputation_config.clone(),
            cached_view: Arc::new(CachedView::new(100, false)),
        };
        let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
        let mut watcher = StateWatcher::from(watch_receiver);

        // when
        let (report_peer_id, report, reporting_service) = tokio::time::timeout(
            Duration::from_secs(1),
            wait_until_report_received(&mut report_receiver, &mut task, &mut watcher),
        )
        .await
        .unwrap();

        // then
        watch_sender.send(State::Stopped).unwrap();

        assert_eq!(
            FuelPeerId::from(peer_id.to_bytes().to_vec()),
            report_peer_id
        );
        assert_eq!(
            report,
            heartbeat_peer_reputation_config.low_heartbeat_frequency_penalty
        );
        assert_eq!(reporting_service, "p2p");
    }

    #[tokio::test]
    async fn peer_heartbeat_reputation_checks__old_heartbeat_sends_reports() {
        // given
        let peer_id = PeerId::random();
        // under the limit
        let last_duration = Duration::from_secs(5);
        let last_heartbeat = Instant::now() - Duration::from_secs(50);
        let last_heartbeat_sys = SystemTime::now() - Duration::from_secs(50);
        let mut durations = VecDeque::new();
        durations.push_front(last_duration);

        let heartbeat_data = HeartbeatData {
            block_height: None,
            last_heartbeat,
            last_heartbeat_sys,
            window: 0,
            durations,
        };
        let peer_info = PeerInfo {
            peer_addresses: Default::default(),
            client_version: None,
            heartbeat_data,
            score: 100.0,
        };
        let peer_info = vec![(peer_id, peer_info)];
        let p2p_service = FakeP2PService {
            peer_info,
            next_event_stream: Box::pin(futures::stream::pending()),
        };
        let (request_sender, request_receiver) = mpsc::channel(100);

        let (report_sender, mut report_receiver) = mpsc::channel(100);
        let broadcast = FakeBroadcast {
            peer_reports: report_sender,
        };

        // Greater than actual
        let heartbeat_max_avg_interval = Duration::from_secs(20);
        // Less than actual
        let heartbeat_max_time_since_last = Duration::from_secs(40);

        // Arbitrary values
        let heartbeat_peer_reputation_config = HeartbeatPeerReputationConfig {
            old_heartbeat_penalty: 5.6,
            low_heartbeat_frequency_penalty: 20.45,
        };

        let mut task = Task {
            chain_id: Default::default(),
            response_timeout: Default::default(),
            p2p_service,
            view_provider: FakeDB,
            tx_pool: FakeTxPool,
            next_block_height: FakeBlockImporter.next_block_height(),
            request_receiver,
            request_sender,
            db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
            tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
            broadcast,
            max_headers_per_request: 0,
            max_txs_per_request: 100,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time: Instant::now(),
            heartbeat_peer_reputation_config: heartbeat_peer_reputation_config.clone(),
            cached_view: Arc::new(CachedView::new(100, false)),
        };
        let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
        let mut watcher = StateWatcher::from(watch_receiver);

        // when
        // we run this in a loop to ensure that the task is run until it reports
        let (report_peer_id, report, reporting_service) = tokio::time::timeout(
            Duration::from_secs(1),
            wait_until_report_received(&mut report_receiver, &mut task, &mut watcher),
        )
        .await
        .unwrap();

        // then
        watch_sender.send(State::Stopped).unwrap();

        assert_eq!(
            FuelPeerId::from(peer_id.to_bytes().to_vec()),
            report_peer_id
        );
        assert_eq!(
            report,
            heartbeat_peer_reputation_config.old_heartbeat_penalty
        );
        assert_eq!(reporting_service, "p2p");
    }

    async fn wait_until_report_received(
        report_receiver: &mut Receiver<(FuelPeerId, AppScore, String)>,
        task: &mut Task<FakeP2PService, FakeDB, FakeBroadcast, FakeTxPool>,
        watcher: &mut StateWatcher,
    ) -> (FuelPeerId, AppScore, String) {
        loop {
            task.run(watcher).await.unwrap();
            if let Ok((peer_id, recv_report, service)) = report_receiver.try_recv() {
                return (peer_id, recv_report, service);
            }
        }
    }

    #[tokio::test]
    async fn should_process_all_imported_block_under_infinite_events_from_p2p() {
        // Given
        let (blocks_processed_sender, mut block_processed_receiver) = mpsc::channel(1);
        let next_block_height = Box::pin(futures::stream::repeat_with(move || {
            blocks_processed_sender.try_send(()).unwrap();
            BlockHeight::from(0)
        }));
        let infinite_event_stream = Box::pin(futures::stream::empty());
        let p2p_service = FakeP2PService {
            peer_info: vec![],
            next_event_stream: infinite_event_stream,
        };

        // Initialization
        let (request_sender, request_receiver) = mpsc::channel(100);
        let broadcast = FakeBroadcast {
            peer_reports: mpsc::channel(100).0,
        };
        let mut task = Task {
            chain_id: Default::default(),
            response_timeout: Default::default(),
            p2p_service,
            tx_pool: FakeTxPool,
            view_provider: FakeDB,
            next_block_height,
            request_receiver,
            request_sender,
            db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
            tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
            broadcast,
            max_headers_per_request: 0,
            max_txs_per_request: 100,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval: Default::default(),
            heartbeat_max_time_since_last: Default::default(),
            next_check_time: Instant::now(),
            heartbeat_peer_reputation_config: Default::default(),
            cached_view: Arc::new(CachedView::new(100, false)),
        };
        let mut watcher = StateWatcher::started();
        // End of initialization

        for _ in 0..100 {
            // When
            task.run(&mut watcher).await.unwrap();

            // Then
            block_processed_receiver
                .try_recv()
                .expect("Should process the block height even under p2p pressure");
        }
    }
}
