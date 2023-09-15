use crate::{
    codecs::{
        postcard::PostcardCodec,
        NetworkCodec,
    },
    config::Config,
    gossipsub::messages::{
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
    },
    request_response::messages::{
        OutboundResponse,
        RequestMessage,
        ResponseChannelItem,
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::ConsensusVote,
        primitives::BlockId,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
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
    },
};
use futures::{
    future::BoxFuture,
    StreamExt,
};
use libp2p::{
    gossipsub::MessageAcceptance,
    PeerId,
};
use libp2p_request_response::RequestId;
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

pub type Service<D> = ServiceRunner<Task<FuelP2PService<PostcardCodec>, D, SharedState>>;

enum TaskRequest {
    // Broadcast requests to p2p network
    BroadcastTransaction(Arc<Transaction>),
    BroadcastBlock(Arc<Block>),
    BroadcastVote(Arc<ConsensusVote>),
    // Request to get one-off data from p2p network
    GetPeerIds(oneshot::Sender<Vec<PeerId>>),
    GetBlock {
        height: BlockHeight,
        channel: oneshot::Sender<Option<SealedBlock>>,
    },
    GetSealedHeaders {
        block_height_range: Range<u32>,
        channel: oneshot::Sender<(PeerId, Option<Vec<SealedBlockHeader>>)>,
    },
    GetTransactions {
        block_id: BlockId,
        from_peer: PeerId,
        channel: oneshot::Sender<Option<Vec<Transaction>>>,
    },
    // Responds back to the p2p network
    RespondWithGossipsubMessageReport((GossipsubMessageInfo, GossipsubMessageAcceptance)),
    RespondWithPeerReport {
        peer_id: PeerId,
        score: AppScore,
        reporting_service: &'static str,
    },
}

impl Debug for TaskRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TaskRequest")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartBeatPeerReportReason {
    OldHeartBeat,
    LowHeartBeatFrequency,
}

impl PeerReport for HeartBeatPeerReportReason {
    fn get_score_from_report(&self) -> AppScore {
        todo!()
    }
}

pub trait TaskP2PService: Send {
    fn get_peer_ids(&self) -> Vec<PeerId>;
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
        channel_item: ResponseChannelItem,
    ) -> anyhow::Result<()>;

    fn send_response_msg(
        &mut self,
        request_id: RequestId,
        message: OutboundResponse,
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
}

impl TaskP2PService for FuelP2PService<PostcardCodec> {
    fn get_peer_ids(&self) -> Vec<PeerId> {
        self.get_peers_ids_iter().copied().collect()
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
        channel_item: ResponseChannelItem,
    ) -> anyhow::Result<()> {
        self.send_request_msg(peer_id, request_msg, channel_item)?;
        Ok(())
    }

    fn send_response_msg(
        &mut self,
        request_id: RequestId,
        message: OutboundResponse,
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
        report: HeartBeatPeerReportReason,
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
        report: HeartBeatPeerReportReason,
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

/// Orchestrates various p2p-related events between the inner `P2pService`
/// and the top level `NetworkService`.
pub struct Task<P, D, B> {
    p2p_service: P,
    db: Arc<D>,
    next_block_height: BoxStream<BlockHeight>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    broadcast: B,
    max_headers_per_request: u32,
    // milliseconds wait time between peer heartbeat reputation checks
    heartbeat_check_interval: Duration,
    heartbeat_max_avg_interval: Duration,
    heartbeat_max_time_since_last: Duration,
    next_check_time: Instant,
}

impl<D> Task<FuelP2PService<PostcardCodec>, D, SharedState> {
    pub fn new<B: BlockHeightImporter>(
        config: Config,
        db: Arc<D>,
        block_importer: Arc<B>,
    ) -> Self {
        let Config {
            max_block_size,
            max_headers_per_request,
            heartbeat_check_interval,
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            ..
        } = config;
        let (request_sender, request_receiver) = mpsc::channel(100);
        let (tx_broadcast, _) = broadcast::channel(100);
        let (block_height_broadcast, _) = broadcast::channel(100);

        let next_block_height = block_importer.next_block_height();
        let p2p_service = FuelP2PService::new(config, PostcardCodec::new(max_block_size));

        let reserved_peers_broadcast =
            p2p_service.peer_manager().reserved_peers_updates();

        let next_check_time = Instant::now() + heartbeat_check_interval;

        Self {
            p2p_service,
            db,
            request_receiver,
            next_block_height,
            broadcast: SharedState {
                request_sender,
                tx_broadcast,
                reserved_peers_broadcast,
                block_height_broadcast,
            },
            max_headers_per_request,
            heartbeat_check_interval,
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time,
        }
    }
}
impl<P: TaskP2PService, D, B: Broadcast> Task<P, D, B> {
    fn peer_heartbeat_reputation_checks(&self) -> anyhow::Result<()> {
        for (peer_id, peer_info) in self.p2p_service.get_all_peer_info() {
            if peer_info.heartbeat_data.duration_since_last_heartbeat()
                > self.heartbeat_max_time_since_last
            {
                tracing::debug!("Peer {:?} has old heartbeat", peer_id);
                let report = HeartBeatPeerReportReason::OldHeartBeat;
                let service = "p2p";
                let peer_id = convert_peer_id(peer_id)?;
                self.broadcast.report_peer(peer_id, report, service)?;
            } else if peer_info.heartbeat_data.average_time_between_heartbeats()
                > self.heartbeat_max_avg_interval
            {
                tracing::debug!("Peer {:?} has low heartbeat frequency", peer_id);
                let report = HeartBeatPeerReportReason::LowHeartBeatFrequency;
                let service = "p2p";
                let peer_id = convert_peer_id(peer_id)?;
                self.broadcast.report_peer(peer_id, report, service)?;
            }
        }
        Ok(())
    }
}

fn convert_peer_id(peer_id: &PeerId) -> anyhow::Result<FuelPeerId> {
    let inner = Vec::try_from(*peer_id)?;
    Ok(FuelPeerId::from(inner))
}

#[async_trait::async_trait]
impl<D> RunnableService for Task<FuelP2PService<PostcardCodec>, D, SharedState>
where
    Self: RunnableTask,
{
    const NAME: &'static str = "P2P";

    type SharedData = SharedState;
    type Task = Task<FuelP2PService<PostcardCodec>, D, SharedState>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.broadcast.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        self.p2p_service.start().await?;
        Ok(self)
    }
}

// TODO: Add tests https://github.com/FuelLabs/fuel-core/issues/1275
#[async_trait::async_trait]
impl<P, D, B> RunnableTask for Task<P, D, B>
where
    P: TaskP2PService + 'static,
    D: P2pDb + 'static,
    B: Broadcast + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        tracing::debug!("P2P task is running");
        let should_continue;

        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            }

            next_service_request = self.request_receiver.recv() => {
                should_continue = true;
                match next_service_request {
                    Some(TaskRequest::BroadcastTransaction(transaction)) => {
                        let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                        let result = self.p2p_service.publish_message(broadcast);
                        if let Err(e) = result {
                            tracing::error!("Got an error during transaction broadcasting {}", e);
                        }
                    }
                    Some(TaskRequest::BroadcastBlock(block)) => {
                        let broadcast = GossipsubBroadcastRequest::NewBlock(block);
                        let result = self.p2p_service.publish_message(broadcast);
                        if let Err(e) = result {
                            tracing::error!("Got an error during block broadcasting {}", e);
                        }
                    }
                    Some(TaskRequest::BroadcastVote(vote)) => {
                        let broadcast = GossipsubBroadcastRequest::ConsensusVote(vote);
                        let result = self.p2p_service.publish_message(broadcast);
                        if let Err(e) = result {
                            tracing::error!("Got an error during vote broadcasting {}", e);
                        }
                    }
                    Some(TaskRequest::GetPeerIds(channel)) => {
                        let peer_ids = self.p2p_service.get_peer_ids();
                        let _ = channel.send(peer_ids);
                    }
                    Some(TaskRequest::GetBlock { height, channel }) => {
                        let request_msg = RequestMessage::Block(height);
                        let channel_item = ResponseChannelItem::Block(channel);
                        let peer = self.p2p_service.get_peer_id_with_height(&height);
                        let _ = self.p2p_service.send_request_msg(peer, request_msg, channel_item);
                    }
                    Some(TaskRequest::GetSealedHeaders { block_height_range, channel: response}) => {
                        let request_msg = RequestMessage::SealedHeaders(block_height_range.clone());
                        let channel_item = ResponseChannelItem::SealedHeaders(response);

                        // Note: this range has already been check for
                        // validity in `SharedState::get_sealed_block_headers`.
                        let block_height = BlockHeight::from(block_height_range.end - 1);
                        let peer = self.p2p_service
                             .get_peer_id_with_height(&block_height);
                        let _ = self.p2p_service.send_request_msg(peer, request_msg, channel_item);
                    }
                    Some(TaskRequest::GetTransactions { block_id, from_peer, channel }) => {
                        let request_msg = RequestMessage::Transactions(block_id);
                        let channel_item = ResponseChannelItem::Transactions(channel);
                        let _ = self.p2p_service.send_request_msg(Some(from_peer), request_msg, channel_item);
                    }
                    Some(TaskRequest::RespondWithGossipsubMessageReport((message, acceptance))) => {
                        // report_message(&mut self.p2p_service, message, acceptance);
                        self.p2p_service.report_message(message, acceptance)?;
                    }
                    Some(TaskRequest::RespondWithPeerReport { peer_id, score, reporting_service }) => {
                        let _ = self.p2p_service.report_peer(peer_id, score, reporting_service);
                    }
                    None => {
                        unreachable!("The `Task` is holder of the `Sender`, so it should not be possible");
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
                            GossipsubMessage::NewBlock(block) => {
                                // todo: add logic to gossip newly received blocks
                                let _new_block = GossipData::new(block, peer_id, message_id);
                            },
                            GossipsubMessage::ConsensusVote(vote) => {
                                // todo: add logic to gossip newly received votes
                                let _new_vote = GossipData::new(vote, peer_id, message_id);
                            },
                        }
                    },
                    Some(FuelP2PEvent::RequestMessage { request_message, request_id }) => {
                        match request_message {
                            RequestMessage::Block(block_height) => {
                                match self.db.get_sealed_block(&block_height) {
                                    Ok(maybe_block) => {
                                        let response = maybe_block.map(Arc::new);
                                        let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::Block(response));
                                    },
                                    Err(e) => {
                                        tracing::error!("Failed to get block at height {:?}: {:?}", block_height, e);
                                        let response = None;
                                        let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::Block(response));
                                        return Err(e.into())
                                    }
                                }
                            }
                            RequestMessage::Transactions(block_id) => {
                                match self.db.get_transactions(&block_id) {
                                    Ok(maybe_transactions) => {
                                        let response = maybe_transactions.map(Arc::new);
                                        let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::Transactions(response));
                                    },
                                    Err(e) => {
                                        tracing::error!("Failed to get transactions for block {:?}: {:?}", block_id, e);
                                        let response = None;
                                        let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::Transactions(response));
                                        return Err(e.into())
                                    }
                                }
                            }
                            RequestMessage::SealedHeaders(range) => {
                                let max_len = self.max_headers_per_request.try_into().expect("u32 should always fit into usize");
                                if range.len() > max_len {
                                    tracing::error!("Requested range of sealed headers is too big. Requested length: {:?}, Max length: {:?}", range.len(), max_len);
                                    // TODO: Return helpful error message to requester. https://github.com/FuelLabs/fuel-core/issues/1311
                                    let response = None;
                                    let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::SealedHeaders(response));
                                } else {
                                    match self.db.get_sealed_headers(range.clone()) {
                                        Ok(headers) => {
                                            let response = Some(headers);
                                            let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::SealedHeaders(response));
                                        },
                                        Err(e) => {
                                            tracing::error!("Failed to get sealed headers for range {:?}: {:?}", range, &e);
                                            let response = None;
                                            let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::SealedHeaders(response));
                                            return Err(e.into())
                                        }
                                    }
                                };
                            }
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
            },
            latest_block_height = self.next_block_height.next() => {
                if let Some(latest_block_height) = latest_block_height {
                    let _ = self.p2p_service.update_block_height(latest_block_height);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
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

    pub async fn get_block(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SealedBlock>> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(TaskRequest::GetBlock {
                height,
                channel: sender,
            })
            .await?;

        receiver.await.map_err(|e| anyhow!("{}", e))
    }

    pub async fn get_sealed_block_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> anyhow::Result<(Vec<u8>, Option<Vec<SealedBlockHeader>>)> {
        let (sender, receiver) = oneshot::channel();

        if block_height_range.is_empty() {
            return Err(anyhow!(
                "Cannot retrieve headers for an empty range of block heights"
            ))
        }

        self.request_sender
            .send(TaskRequest::GetSealedHeaders {
                block_height_range,
                channel: sender,
            })
            .await?;

        receiver
            .await
            .map(|(peer_id, headers)| (peer_id.to_bytes(), headers))
            .map_err(|e| anyhow!("{}", e))
    }

    pub async fn get_transactions_from_peer(
        &self,
        peer_id: Vec<u8>,
        block_id: BlockId,
    ) -> anyhow::Result<Option<Vec<Transaction>>> {
        let (sender, receiver) = oneshot::channel();
        let from_peer = PeerId::from_bytes(&peer_id).expect("Valid PeerId");

        self.request_sender
            .send(TaskRequest::GetTransactions {
                block_id,
                from_peer,
                channel: sender,
            })
            .await?;

        receiver.await.map_err(|e| anyhow!("{}", e))
    }

    pub fn broadcast_vote(&self, vote: Arc<ConsensusVote>) -> anyhow::Result<()> {
        self.request_sender
            .try_send(TaskRequest::BroadcastVote(vote))?;

        Ok(())
    }

    pub fn broadcast_block(&self, block: Arc<Block>) -> anyhow::Result<()> {
        self.request_sender
            .try_send(TaskRequest::BroadcastBlock(block))?;

        Ok(())
    }

    pub fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        self.request_sender
            .try_send(TaskRequest::BroadcastTransaction(transaction))?;
        Ok(())
    }

    pub async fn get_peer_ids(&self) -> anyhow::Result<Vec<PeerId>> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(TaskRequest::GetPeerIds(sender))
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

pub fn new_service<D, B>(p2p_config: Config, db: D, block_importer: B) -> Service<D>
where
    D: P2pDb + 'static,
    B: BlockHeightImporter,
{
    Service::new(Task::new(
        p2p_config,
        Arc::new(db),
        Arc::new(block_importer),
    ))
}

pub(crate) fn to_message_acceptance(
    acceptance: &GossipsubMessageAcceptance,
) -> MessageAcceptance {
    match acceptance {
        GossipsubMessageAcceptance::Accept => MessageAcceptance::Accept,
        GossipsubMessageAcceptance::Reject => MessageAcceptance::Reject,
        GossipsubMessageAcceptance::Ignore => MessageAcceptance::Ignore,
    }
}

fn report_message<T: NetworkCodec>(
    p2p_service: &mut FuelP2PService<T>,
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
    use fuel_core_types::fuel_types::BlockHeight;
    use futures::FutureExt;
    use std::collections::VecDeque;

    #[derive(Clone, Debug)]
    struct FakeDb;

    impl P2pDb for FakeDb {
        fn get_sealed_block(
            &self,
            _height: &BlockHeight,
        ) -> StorageResult<Option<SealedBlock>> {
            unimplemented!()
        }

        fn get_sealed_header(
            &self,
            _height: &BlockHeight,
        ) -> StorageResult<Option<SealedBlockHeader>> {
            unimplemented!()
        }

        fn get_sealed_headers(
            &self,
            _block_height_range: Range<u32>,
        ) -> StorageResult<Vec<SealedBlockHeader>> {
            unimplemented!()
        }

        fn get_transactions(
            &self,
            _block_id: &fuel_core_types::blockchain::primitives::BlockId,
        ) -> StorageResult<Option<Vec<Transaction>>> {
            unimplemented!()
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
        let p2p_config = Config::default_initialized("start_stop_works");
        let service = new_service(p2p_config, FakeDb, FakeBlockImporter);

        // Node with p2p service started
        assert!(service.start_and_await().await.unwrap().started());
        // Node with p2p service stopped
        assert!(service.stop_and_await().await.unwrap().stopped());
    }

    struct FakeP2PService {
        peer_info: Vec<(PeerId, PeerInfo)>,
    }

    impl TaskP2PService for FakeP2PService {
        fn get_peer_ids(&self) -> Vec<PeerId> {
            todo!()
        }

        fn get_all_peer_info(&self) -> Vec<(&PeerId, &PeerInfo)> {
            self.peer_info
                .iter()
                .map(|(peer_id, peer_info)| (peer_id, peer_info))
                .collect()
        }

        fn get_peer_id_with_height(&self, _height: &BlockHeight) -> Option<PeerId> {
            todo!()
        }

        fn next_event(&mut self) -> BoxFuture<'_, Option<FuelP2PEvent>> {
            std::future::pending().boxed()
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
            _channel_item: ResponseChannelItem,
        ) -> anyhow::Result<()> {
            todo!()
        }

        fn send_response_msg(
            &mut self,
            _request_id: RequestId,
            _message: OutboundResponse,
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
            todo!()
        }
    }

    struct FakeDB;

    impl P2pDb for FakeDB {
        fn get_sealed_block(
            &self,
            _height: &BlockHeight,
        ) -> StorageResult<Option<SealedBlock>> {
            todo!()
        }

        fn get_sealed_header(
            &self,
            _height: &BlockHeight,
        ) -> StorageResult<Option<SealedBlockHeader>> {
            todo!()
        }

        fn get_sealed_headers(
            &self,
            _block_height_range: Range<u32>,
        ) -> StorageResult<Vec<SealedBlockHeader>> {
            todo!()
        }

        fn get_transactions(
            &self,
            _block_id: &BlockId,
        ) -> StorageResult<Option<Vec<Transaction>>> {
            todo!()
        }
    }

    struct FakeBroadcast {
        pub peer_reports: mpsc::Sender<(FuelPeerId, HeartBeatPeerReportReason, String)>,
    }

    impl Broadcast for FakeBroadcast {
        fn report_peer(
            &self,
            peer_id: FuelPeerId,
            report: HeartBeatPeerReportReason,
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
        let p2p_service = FakeP2PService { peer_info };
        let (_request_sender, request_receiver) = mpsc::channel(100);

        let (report_sender, mut report_receiver) = mpsc::channel(100);
        let broadcast = FakeBroadcast {
            peer_reports: report_sender,
        };

        // Less than actual
        let heartbeat_max_avg_interval = Duration::from_secs(20);
        // Greater than actual
        let heartbeat_max_time_since_last = Duration::from_secs(40);

        let mut task = Task {
            p2p_service,
            db: Arc::new(FakeDB),
            next_block_height: FakeBlockImporter.next_block_height(),
            request_receiver,
            broadcast,
            max_headers_per_request: 0,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time: Instant::now(),
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
        assert_eq!(report, HeartBeatPeerReportReason::LowHeartBeatFrequency);
        assert_eq!(reporting_service, "p2p");
    }

    #[tokio::test]
    async fn peer_heartbeat_reputation_checks__old_heartbeat_sends_reports() {
        // given
        let peer_id = PeerId::random();
        // under the limit
        let last_duration = Duration::from_secs(5);
        let last_heartbeat = Instant::now() - Duration::from_secs(50);
        let mut durations = VecDeque::new();
        durations.push_front(last_duration);

        let heartbeat_data = HeartbeatData {
            block_height: None,
            last_heartbeat,
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
        let p2p_service = FakeP2PService { peer_info };
        let (_request_sender, request_receiver) = mpsc::channel(100);

        let (report_sender, mut report_receiver) = mpsc::channel(100);
        let broadcast = FakeBroadcast {
            peer_reports: report_sender,
        };

        // Greater than actual
        let heartbeat_max_avg_interval = Duration::from_secs(20);
        // Less than actual
        let heartbeat_max_time_since_last = Duration::from_secs(40);

        let mut task = Task {
            p2p_service,
            db: Arc::new(FakeDB),
            next_block_height: FakeBlockImporter.next_block_height(),
            request_receiver,
            broadcast,
            max_headers_per_request: 0,
            heartbeat_check_interval: Duration::from_secs(0),
            heartbeat_max_avg_interval,
            heartbeat_max_time_since_last,
            next_check_time: Instant::now(),
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
        assert_eq!(report, HeartBeatPeerReportReason::OldHeartBeat);
        assert_eq!(reporting_service, "p2p");
    }
}
