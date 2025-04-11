use crate::{
    cached_view::CachedView,
    codecs::{
        gossipsub::GossipsubMessageHandler,
        request_response::RequestResponseMessageHandler,
    },
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
        P2PPreConfirmationGossipData,
        P2PPreConfirmationMessage,
        P2pDb,
        TxPool,
    },
    request_response::messages::{
        OnResponse,
        OnResponseWithPeerSelection,
        RequestMessage,
        ResponseMessageErrorCode,
        ResponseSender,
        V2ResponseMessage,
    },
};
use anyhow::anyhow;
use fuel_core_metrics::p2p_metrics::set_blocks_requested;
use fuel_core_services::{
    AsyncProcessor,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    SyncProcessor,
    TaskNextAction,
    TraceErr,
    stream::BoxStream,
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
        BlockHeightHeartbeatData,
        GossipData,
        GossipsubMessageAcceptance,
        GossipsubMessageInfo,
        NetworkableTransactionPool,
        PeerId as FuelPeerId,
        TransactionGossipData,
        Transactions,
        peer_reputation::{
            AppScore,
            PeerReport,
        },
    },
};
use futures::{
    StreamExt,
    future::BoxFuture,
};
use libp2p::{
    PeerId,
    gossipsub::{
        MessageAcceptance,
        MessageId,
        PublishError,
    },
    request_response::InboundRequestId,
};
use std::{
    fmt::Debug,
    future::Future,
    ops::Range,
    sync::Arc,
};
use thiserror::Error;
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

#[cfg(test)]
pub mod broadcast_tests;
#[cfg(test)]
pub mod task_tests;

const CHANNEL_SIZE: usize = 1024 * 10;

pub type Service<V, T> = ServiceRunner<UninitializedTask<V, SharedState, T>>;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("No peer found to send request to")]
    NoPeerFound,
}

pub enum TaskRequest {
    // Broadcast requests to p2p network
    BroadcastTransaction(Arc<Transaction>),
    // Broadcast Preconfirmations to p2p network
    BroadcastPreConfirmations(Arc<P2PPreConfirmationMessage>),
    // Request to get information about all connected peers
    GetAllPeerInfo {
        channel: oneshot::Sender<Vec<(PeerId, PeerInfo)>>,
    },
    GetSealedHeaders {
        block_height_range: Range<u32>,
        channel: OnResponseWithPeerSelection<
            Result<Vec<SealedBlockHeader>, ResponseMessageErrorCode>,
        >,
    },
    GetTransactions {
        block_height_range: Range<u32>,
        channel: OnResponseWithPeerSelection<
            Result<Vec<Transactions>, ResponseMessageErrorCode>,
        >,
    },
    GetTransactionsFromPeer {
        block_height_range: Range<u32>,
        from_peer: PeerId,
        channel: OnResponse<Result<Vec<Transactions>, ResponseMessageErrorCode>>,
    },
    TxPoolGetAllTxIds {
        from_peer: PeerId,
        channel: OnResponse<Result<Vec<TxId>, ResponseMessageErrorCode>>,
    },
    TxPoolGetFullTransactions {
        tx_ids: Vec<TxId>,
        from_peer: PeerId,
        channel: OnResponse<
            Result<Vec<Option<NetworkableTransactionPool>>, ResponseMessageErrorCode>,
        >,
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
            TaskRequest::BroadcastPreConfirmations(_) => {
                write!(f, "TaskRequest::BroadcastPreConfirmations")
            }
            TaskRequest::GetSealedHeaders { .. } => {
                write!(f, "TaskRequest::GetSealedHeaders")
            }
            TaskRequest::GetTransactions { .. } => {
                write!(f, "TaskRequest::GetTransactions")
            }
            TaskRequest::GetTransactionsFromPeer { .. } => {
                write!(f, "TaskRequest::GetTransactionsFromPeer")
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

    fn pre_confirmation_broadcast(
        &self,
        confirmations: P2PPreConfirmationGossipData,
    ) -> anyhow::Result<()>;

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

    fn pre_confirmation_broadcast(
        &self,
        confirmations: P2PPreConfirmationGossipData,
    ) -> anyhow::Result<()> {
        self.pre_confirmations_broadcast.send(confirmations)?;
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
    last_height: BlockHeight,
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

impl<P, V, B, T> Task<P, V, B, T>
where
    P: TaskP2PService,
    B: Broadcast,
{
    pub(crate) fn broadcast_gossip_message(
        &mut self,
        message: GossipsubMessage,
        message_id: MessageId,
        peer_id: PeerId,
    ) {
        let message_id = message_id.0;

        match message {
            GossipsubMessage::NewTx(transaction) => {
                let next_transaction = GossipData::new(transaction, peer_id, message_id);
                let _ = self.broadcast.tx_broadcast(next_transaction);
            }
            GossipsubMessage::TxPreConfirmations(confirmations) => {
                let data = GossipData::new(confirmations, peer_id, message_id);
                let _ = self.broadcast.pre_confirmation_broadcast(data);
            }
        }
    }
}

#[derive(Default, Clone)]
pub struct HeartbeatPeerReputationConfig {
    old_heartbeat_penalty: AppScore,
    low_heartbeat_frequency_penalty: AppScore,
}

impl<V, T> UninitializedTask<V, SharedState, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new<B: BlockHeightImporter>(
        chain_id: ChainId,
        last_height: BlockHeight,
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
            last_height,
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
                "Requested range is too large"
            );
            let response = Err(ResponseMessageErrorCode::RequestedRangeTooLarge);
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

                let response = db_lookup(&view, &cached_view, range.clone())
                    .ok()
                    .flatten()
                    .ok_or(ResponseMessageErrorCode::Timeout);

                let _ = response_channel
                    .try_send(task_request(response, request_id))
                    .trace_err("Failed to send response to the request channel");
            }
        });

        if result.is_err() {
            let err = Err(ResponseMessageErrorCode::SyncProcessorOutOfCapacity);
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

            let _ = response_channel
                .try_send(task_request(Ok(response), request_id))
                .trace_err("Failed to send response to the request channel");
        });

        if result.is_err() {
            let res = Err(ResponseMessageErrorCode::SyncProcessorOutOfCapacity);
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
        if tx_ids.len() > self.max_txs_per_request {
            self.p2p_service.send_response_msg(
                request_id,
                V2ResponseMessage::TxPoolFullTransactions(Err(
                    ResponseMessageErrorCode::RequestedRangeTooLarge,
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
            last_height,
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
            GossipsubMessageHandler::new(),
            RequestResponseMessageHandler::new(max_block_size),
        )
        .await?;
        p2p_service.update_block_height(last_height);
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
impl<P, V, B, T> RunnableTask for Task<P, V, B, T>
where
    P: TaskP2PService + 'static,
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    B: Broadcast + 'static,
    T: TxPool + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                TaskNextAction::Stop
            },
            latest_block_height = self.next_block_height.next() => {
                if let Some(latest_block_height) = latest_block_height {
                    let _ = self.p2p_service.update_block_height(latest_block_height);
                    TaskNextAction::Continue
                } else {
                    TaskNextAction::Stop
                }
            },
            next_service_request = self.request_receiver.recv() => {
                match next_service_request {
                    Some(TaskRequest::BroadcastTransaction(transaction)) => {
                        let tx_id = transaction.id(&self.chain_id);
                        let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                        let result = self.p2p_service.publish_message(broadcast);
                        if let Err(e) = result {
                            tracing::error!("Got an error during transaction {} broadcasting {}", tx_id, e);
                        }
                    }
                    Some(TaskRequest::BroadcastPreConfirmations(pre_confirmation_message)) => {
                        let broadcast = GossipsubBroadcastRequest::TxPreConfirmations(pre_confirmation_message);
                        let result = self.p2p_service.publish_message(broadcast);
                        if let Err(e) = result {
                            tracing::error!("Got an error during pre-confirmation message broadcasting {}", e);
                        }
                    }
                    Some(TaskRequest::GetSealedHeaders { block_height_range, channel}) => {
                        // Note: this range has already been checked for
                        // validity in `SharedState::get_sealed_block_headers`.
                        let height = BlockHeight::from(block_height_range.end.saturating_sub(1));
                        let Some(peer) = self.p2p_service.get_peer_id_with_height(&height) else {
                            let _ = channel.send(Err(TaskError::NoPeerFound));
                            return TaskNextAction::Continue
                        };
                        let channel = ResponseSender::SealedHeaders(channel);
                        let request_msg = RequestMessage::SealedHeaders(block_height_range.clone());
                        self.p2p_service.send_request_msg(Some(peer), request_msg, channel).expect("We always have a peer here, so send has a target");
                    }
                    Some(TaskRequest::GetTransactions {block_height_range, channel }) => {
                        let height = BlockHeight::from(block_height_range.end.saturating_sub(1));
                        let Some(peer) = self.p2p_service.get_peer_id_with_height(&height) else {
                            let _ = channel.send(Err(TaskError::NoPeerFound));
                            return TaskNextAction::Continue
                        };
                        let channel = ResponseSender::Transactions(channel);
                        let request_msg = RequestMessage::Transactions(block_height_range.clone());
                        self.p2p_service.send_request_msg(Some(peer), request_msg, channel).expect("We always have a peer here, so send has a target");
                    }
                    Some(TaskRequest::GetTransactionsFromPeer { block_height_range, from_peer, channel }) => {
                        let channel = ResponseSender::TransactionsFromPeer(channel);
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
                        let res = self.p2p_service.report_message(message, acceptance);
                        if let Err(err) = res {
                            return TaskNextAction::ErrorContinue(err)
                        }
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
                        return TaskNextAction::Stop
                    }
                }
                    TaskNextAction::Continue
            }
            p2p_event = self.p2p_service.next_event() => {
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
                        tracing::info!("Received gossip message from peer {:?}", peer_id);
                        self.broadcast_gossip_message(message, message_id, peer_id);
                    },
                    Some(FuelP2PEvent::InboundRequestMessage { request_message, request_id }) => {
                        let res = self.process_request(request_message, request_id);
                        if let Err(err) = res {
                            return TaskNextAction::ErrorContinue(err)
                        }
                    },
                    Some(FuelP2PEvent::NewSubscription { peer_id, tag }) => {
                        if tag == GossipTopicTag::NewTx {
                            let _ = self.broadcast.new_tx_subscription_broadcast(FuelPeerId::from(peer_id.to_bytes()));
                        }
                    },
                    _ => (),
                }
                TaskNextAction::Continue
            },
            _  = tokio::time::sleep_until(self.next_check_time) => {
                let res = self.peer_heartbeat_reputation_checks();
                match res {
                    Ok(_) => tracing::debug!("Peer heartbeat reputation checks completed"),
                    Err(e) => {
                        tracing::error!("Failed to perform peer heartbeat reputation checks: {:?}", e);
                    }
                }

                if let Some(next_check_time) = self.next_check_time.checked_add(self.heartbeat_check_interval) {
                    self.next_check_time = next_check_time;
                    TaskNextAction::Continue
                } else {
                    tracing::error!("Next check time overflowed");
                    TaskNextAction::Stop
                }
            }
        }
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
    /// Sender of p2p with peer gossip subscription (`Vec<u8>` represent the peer_id)
    new_tx_subscription_broadcast: broadcast::Sender<FuelPeerId>,
    /// Sender of p2p transaction used for subscribing.
    tx_broadcast: broadcast::Sender<TransactionGossipData>,
    /// Sender of p2p transaction preconfirmations used for subscribing.
    pre_confirmations_broadcast: broadcast::Sender<P2PPreConfirmationGossipData>,
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

        let (peer_id, response) = receiver
            .await
            .map_err(|e| anyhow!("{e}"))?
            .map_err(|e| anyhow!("{e}"))?;

        let data = response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))?;
        if let Err(ref response_error_code) = data {
            warn!(
                "Peer {peer_id:?} failed to respond with sealed headers: {response_error_code:?}"
            );
        };

        Ok((peer_id.to_bytes(), data.ok()))
    }

    pub async fn get_transactions(
        &self,
        range: Range<u32>,
    ) -> anyhow::Result<(Vec<u8>, Option<Vec<Transactions>>)> {
        let (sender, receiver) = oneshot::channel();

        if range.is_empty() {
            return Err(anyhow!(
                "Cannot retrieve transactions for an empty range of block heights"
            ));
        }

        self.request_sender
            .send(TaskRequest::GetTransactions {
                block_height_range: range,
                channel: sender,
            })
            .await?;

        let (peer_id, response) = receiver
            .await
            .map_err(|e| anyhow!("{e}"))?
            .map_err(|e| anyhow!("{e}"))?;

        let data = match response {
            Err(request_response_protocol_error) => Err(anyhow!(
                "Invalid response from peer {request_response_protocol_error:?}"
            )),
            Ok(Err(response_error_code)) => {
                warn!(
                    "Peer {peer_id:?} failed to respond with sealed headers: {response_error_code:?}"
                );
                Ok(None)
            }
            Ok(Ok(headers)) => Ok(Some(headers)),
        };
        data.map(|data| (peer_id.to_bytes(), data))
    }

    pub async fn get_transactions_from_peer(
        &self,
        peer_id: FuelPeerId,
        range: Range<u32>,
    ) -> anyhow::Result<Option<Vec<Transactions>>> {
        let (sender, receiver) = oneshot::channel();
        let from_peer = PeerId::from_bytes(peer_id.as_ref()).expect("Valid PeerId");

        let request = TaskRequest::GetTransactionsFromPeer {
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

        match response {
            Err(request_response_protocol_error) => Err(anyhow!(
                "Invalid response from peer {request_response_protocol_error:?}"
            )),
            Ok(Err(response_error_code)) => {
                warn!(
                    "Peer {peer_id:?} failed to respond with transactions: {response_error_code:?}"
                );
                Ok(None)
            }
            Ok(Ok(txs)) => Ok(Some(txs)),
        }
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

        let response =
            response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))?;

        let txs = response.inspect_err(|e| { warn!("Peer {peer_id:?} could not response to request to get all transactions ids: {e:?}"); } ).unwrap_or_default();

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

        let response =
            response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))?;
        let txs = response.inspect_err(|e| { warn!("Peer {peer_id:?} could not response to request to get full transactions: {e:?}"); } ).unwrap_or_default();

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

    pub fn broadcast_preconfirmations(
        &self,
        preconfirmations: Arc<P2PPreConfirmationMessage>,
    ) -> anyhow::Result<()> {
        self.request_sender
            .try_send(TaskRequest::BroadcastPreConfirmations(preconfirmations))?;
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

    pub fn subscribe_preconfirmations(
        &self,
    ) -> broadcast::Receiver<P2PPreConfirmationGossipData> {
        self.pre_confirmations_broadcast.subscribe()
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
    let (preconfirmations_broadcast, _) = broadcast::channel(CHANNEL_SIZE);
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
            pre_confirmations_broadcast: preconfirmations_broadcast,
            reserved_peers_broadcast,
            block_height_broadcast,
            max_txs_per_request: config.max_txs_per_request,
        },
        request_receiver,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn new_service<V, B, T>(
    chain_id: ChainId,
    last_height: BlockHeight,
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
        last_height,
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
