use crate::{
    codecs::postcard::PostcardCodec,
    config::{
        Config,
        NotInitialized,
    },
    gossipsub::messages::{
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    heavy_task_processor::HeavyTaskProcessor,
    p2p_service::{
        FuelP2PEvent,
        FuelP2PService,
    },
    peer_manager::PeerInfo,
    ports::{
        BlockHeightImporter,
        P2pDb,
    },
    request_response::messages::{
        OnResponse,
        RequestMessage,
        ResponseMessage,
        ResponseSender,
    },
};
use anyhow::anyhow;
use fuel_core_metrics::p2p_metrics::set_blocks_requested;
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TraceErr,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::SealedBlockHeader,
    fuel_tx::{
        Transaction,
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
    gossipsub::MessageAcceptance,
    request_response::InboundRequestId,
    PeerId,
};
use std::{
    fmt::Debug,
    ops::Range,
    sync::Arc,
};
use tokio::{
    sync::{
        broadcast,
        mpsc,
        oneshot,
    },
    time::{
        Duration,
        Instant,
    },
};
use tracing::warn;

pub type Service<V> = ServiceRunner<UninitializedTask<V, SharedState>>;

enum TaskRequest {
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
    // Responds back to the p2p network
    RespondWithGossipsubMessageReport((GossipsubMessageInfo, GossipsubMessageAcceptance)),
    RespondWithPeerReport {
        peer_id: PeerId,
        score: AppScore,
        reporting_service: &'static str,
    },
    DatabaseTransactionsLookUp {
        response: Option<Vec<Transactions>>,
        request_id: InboundRequestId,
    },
    DatabaseHeaderLookUp {
        response: Option<Vec<SealedBlockHeader>>,
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
        message: ResponseMessage,
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
        self.publish_message(message)?;
        Ok(())
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
        message: ResponseMessage,
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
}

/// Uninitialized task for the p2p that can be upgraded later into [`Task`].
pub struct UninitializedTask<V, B> {
    chain_id: ChainId,
    view_provider: V,
    next_block_height: BoxStream<BlockHeight>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    broadcast: B,
    config: Config<NotInitialized>,
}

/// Orchestrates various p2p-related events between the inner `P2pService`
/// and the top level `NetworkService`.
pub struct Task<P, V, B> {
    chain_id: ChainId,
    response_timeout: Duration,
    p2p_service: P,
    view_provider: V,
    next_block_height: BoxStream<BlockHeight>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    request_sender: mpsc::Sender<TaskRequest>,
    database_processor: HeavyTaskProcessor,
    broadcast: B,
    max_headers_per_request: usize,
    // milliseconds wait time between peer heartbeat reputation checks
    heartbeat_check_interval: Duration,
    heartbeat_max_avg_interval: Duration,
    heartbeat_max_time_since_last: Duration,
    next_check_time: Instant,
    heartbeat_peer_reputation_config: HeartbeatPeerReputationConfig,
}

#[derive(Default, Clone)]
pub struct HeartbeatPeerReputationConfig {
    old_heartbeat_penalty: AppScore,
    low_heartbeat_frequency_penalty: AppScore,
}

impl<V> UninitializedTask<V, SharedState> {
    pub fn new<B: BlockHeightImporter>(
        chain_id: ChainId,
        config: Config<NotInitialized>,
        view_provider: V,
        block_importer: B,
    ) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(1024 * 10);
        let (tx_broadcast, _) = broadcast::channel(1024 * 10);
        let (block_height_broadcast, _) = broadcast::channel(1024 * 10);

        let (reserved_peers_broadcast, _) = broadcast::channel::<usize>(
            config
                .reserved_nodes
                .len()
                .saturating_mul(2)
                .saturating_add(1),
        );
        let next_block_height = block_importer.next_block_height();

        Self {
            chain_id,
            view_provider,
            next_block_height,
            request_receiver,
            broadcast: SharedState {
                request_sender,
                tx_broadcast,
                reserved_peers_broadcast,
                block_height_broadcast,
            },
            config,
        }
    }
}

impl<P: TaskP2PService, V, B: Broadcast> Task<P, V, B> {
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

impl<P, V, B> Task<P, V, B>
where
    P: TaskP2PService + 'static,
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
{
    fn update_metrics<T>(&self, update_fn: T)
    where
        T: FnOnce(),
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
        }
    }

    fn handle_request<DbLookUpFn, ResponseSenderFn, TaskRequestFn, R>(
        &mut self,
        range: Range<u32>,
        request_id: InboundRequestId,
        response_sender: ResponseSenderFn,
        db_lookup: DbLookUpFn,
        task_request: TaskRequestFn,
    ) -> anyhow::Result<()>
    where
        DbLookUpFn:
            Fn(&V::LatestView, Range<u32>) -> anyhow::Result<Option<R>> + Send + 'static,
        ResponseSenderFn: Fn(Option<R>) -> ResponseMessage + Send + 'static,
        TaskRequestFn: Fn(Option<R>, InboundRequestId) -> TaskRequest + Send + 'static,
        R: Send + 'static,
    {
        let instant = Instant::now();
        let timeout = self.response_timeout;
        let response_channel = self.request_sender.clone();
        // For now, we only process requests that are smaller than the max_blocks_per_request
        // If there are other types of data we send over p2p req/res protocol, then this needs
        // to be generalized
        let max_len = self.max_headers_per_request;
        let range_len = range.len();

        self.update_metrics(|| set_blocks_requested(range_len));

        if range_len > max_len {
            tracing::error!(
                requested_length = range.len(),
                max_len,
                "Requested range is too big"
            );
            // TODO: Return helpful error message to requester. https://github.com/FuelLabs/fuel-core/issues/1311
            let response = None;
            let _ = self
                .p2p_service
                .send_response_msg(request_id, response_sender(response));
            return Ok(());
        }

        let view = self.view_provider.latest_view()?;
        let result = self.database_processor.spawn(move || {
            if instant.elapsed() > timeout {
                tracing::warn!("Request timed out");
                return;
            }

            let response = db_lookup(&view, range.clone()).ok().flatten();

            let _ = response_channel
                .try_send(task_request(response, request_id))
                .trace_err("Failed to send response to the request channel");
        });

        if result.is_err() {
            let _ = self
                .p2p_service
                .send_response_msg(request_id, response_sender(None));
        }

        Ok(())
    }

    fn handle_transactions_request(
        &mut self,
        range: Range<u32>,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        self.handle_request(
            range,
            request_id,
            ResponseMessage::Transactions,
            |view, range| view.get_transactions(range).map_err(anyhow::Error::from),
            |response, request_id| TaskRequest::DatabaseTransactionsLookUp {
                response,
                request_id,
            },
        )
    }

    fn handle_sealed_headers_request(
        &mut self,
        range: Range<u32>,
        request_id: InboundRequestId,
    ) -> anyhow::Result<()> {
        self.handle_request(
            range,
            request_id,
            ResponseMessage::SealedHeaders,
            |view, range| view.get_sealed_headers(range).map_err(anyhow::Error::from),
            |response, request_id| TaskRequest::DatabaseHeaderLookUp {
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
impl<V> RunnableService for UninitializedTask<V, SharedState>
where
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
{
    const NAME: &'static str = "P2P";

    type SharedData = SharedState;
    type Task = Task<FuelP2PService, V, SharedState>;
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
            config,
        } = self;

        let view = view_provider.latest_view()?;
        let genesis = view.get_genesis()?;
        let config = config.init(genesis)?;
        let Config {
            max_block_size,
            max_headers_per_request,
            heartbeat_check_interval,
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
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
        );
        p2p_service.start().await?;

        let next_check_time =
            Instant::now().checked_add(heartbeat_check_interval).expect(
                "The heartbeat check interval should be small enough to do frequently",
            );
        let number_of_threads = 2;
        let database_processor = HeavyTaskProcessor::new(number_of_threads, 1024 * 10)?;
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
            database_processor,
            max_headers_per_request,
            heartbeat_check_interval,
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time,
            heartbeat_peer_reputation_config,
        };
        Ok(task)
    }
}

// TODO: Add tests https://github.com/FuelLabs/fuel-core/issues/1275
#[async_trait::async_trait]
impl<P, V, B> RunnableTask for Task<P, V, B>
where
    P: TaskP2PService + 'static,
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    B: Broadcast + 'static,
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
                        let _ = self.p2p_service.send_response_msg(request_id, ResponseMessage::Transactions(response));
                    }
                    Some(TaskRequest::DatabaseHeaderLookUp { response, request_id }) => {
                        let _ = self.p2p_service.send_response_msg(request_id, ResponseMessage::SealedHeaders(response));
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
    /// Sender of p2p transaction used for subscribing.
    tx_broadcast: broadcast::Sender<TransactionGossipData>,
    /// Sender of reserved peers connection updates.
    reserved_peers_broadcast: broadcast::Sender<usize>,
    /// Used for communicating with the `Task`.
    request_sender: mpsc::Sender<TaskRequest>,
    /// Sender of p2p blopck height data
    block_height_broadcast: broadcast::Sender<BlockHeightHeartbeatData>,
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
        peer_id: Vec<u8>,
        range: Range<u32>,
    ) -> anyhow::Result<Option<Vec<Transactions>>> {
        let (sender, receiver) = oneshot::channel();
        let from_peer = PeerId::from_bytes(&peer_id).expect("Valid PeerId");

        let request = TaskRequest::GetTransactions {
            block_height_range: range,
            from_peer,
            channel: sender,
        };
        self.request_sender.send(request).await?;

        let (response_from_peer, response) =
            receiver.await.map_err(|e| anyhow!("{e}"))?;
        assert_eq!(
            peer_id,
            response_from_peer.to_bytes(),
            "Bug: response from non-requested peer"
        );

        response.map_err(|e| anyhow!("Invalid response from peer {e:?}"))
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

pub fn new_service<V, B>(
    chain_id: ChainId,
    p2p_config: Config<NotInitialized>,
    view_provider: V,
    block_importer: B,
) -> Service<V>
where
    V: AtomicView + 'static,
    V::LatestView: P2pDb,
    B: BlockHeightImporter,
{
    let task =
        UninitializedTask::new(chain_id, p2p_config, view_provider, block_importer);
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

    #[tokio::test]
    async fn start_and_stop_awaits_works() {
        let p2p_config = Config::<NotInitialized>::default("start_stop_works");
        let service =
            new_service(ChainId::default(), p2p_config, FakeDb, FakeBlockImporter);

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
            _message: ResponseMessage,
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
            request_receiver,
            request_sender,
            database_processor: HeavyTaskProcessor::new(1, 1).unwrap(),
            broadcast,
            max_headers_per_request: 0,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time: Instant::now(),
            heartbeat_peer_reputation_config: heartbeat_peer_reputation_config.clone(),
        };
        let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
        let mut watcher = StateWatcher::from(watch_receiver);

        // when
        task.run(&mut watcher).await.unwrap();

        // then
        let (report_peer_id, report, reporting_service) =
            report_receiver.recv().await.unwrap();

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
            next_block_height: FakeBlockImporter.next_block_height(),
            request_receiver,
            request_sender,
            database_processor: HeavyTaskProcessor::new(1, 1).unwrap(),
            broadcast,
            max_headers_per_request: 0,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time: Instant::now(),
            heartbeat_peer_reputation_config: heartbeat_peer_reputation_config.clone(),
        };
        let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
        let mut watcher = StateWatcher::from(watch_receiver);

        // when
        task.run(&mut watcher).await.unwrap();

        // then
        let (report_peer_id, report, reporting_service) =
            report_receiver.recv().await.unwrap();

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
            view_provider: FakeDB,
            next_block_height,
            request_receiver,
            request_sender,
            database_processor: HeavyTaskProcessor::new(1, 1).unwrap(),
            broadcast,
            max_headers_per_request: 0,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval: Default::default(),
            heartbeat_max_time_since_last: Default::default(),
            next_check_time: Instant::now(),
            heartbeat_peer_reputation_config: Default::default(),
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
