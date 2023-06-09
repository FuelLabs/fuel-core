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
use futures::StreamExt;
use libp2p::{
    gossipsub::MessageAcceptance,
    PeerId,
};
use std::{
    fmt::Debug,
    sync::Arc,
};
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot,
};
use tracing::warn;

pub type Service<D> = ServiceRunner<Task<D>>;

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
    GetSealedHeader {
        height: BlockHeight,
        channel: oneshot::Sender<Option<(PeerId, SealedBlockHeader)>>,
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

/// Orchestrates various p2p-related events between the inner `P2pService`
/// and the top level `NetworkService`.
pub struct Task<D> {
    p2p_service: FuelP2PService<PostcardCodec>,
    db: Arc<D>,
    next_block_height: BoxStream<BlockHeight>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    shared: SharedState,
}

impl<D> Task<D> {
    pub fn new<B: BlockHeightImporter>(
        config: Config,
        db: Arc<D>,
        block_importer: Arc<B>,
    ) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(100);
        let (tx_broadcast, _) = broadcast::channel(100);
        let (block_height_broadcast, _) = broadcast::channel(100);

        let next_block_height = block_importer.next_block_height();
        let max_block_size = config.max_block_size;
        let p2p_service = FuelP2PService::new(config, PostcardCodec::new(max_block_size));

        let reserved_peers_broadcast =
            p2p_service.peer_manager().reserved_peers_updates();

        Self {
            p2p_service,
            db,
            request_receiver,
            next_block_height,
            shared: SharedState {
                request_sender,
                tx_broadcast,
                reserved_peers_broadcast,
                block_height_broadcast,
            },
        }
    }
}

#[async_trait::async_trait]
impl<D> RunnableService for Task<D>
where
    Self: RunnableTask,
{
    const NAME: &'static str = "P2P";

    type SharedData = SharedState;
    type Task = Task<D>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        self.p2p_service.start()?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<D> RunnableTask for Task<D>
where
    D: P2pDb + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
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
                        let peer_ids = self.p2p_service.get_peers_ids().copied().collect();
                        let _ = channel.send(peer_ids);
                    }
                    Some(TaskRequest::GetBlock { height, channel }) => {
                        let request_msg = RequestMessage::Block(height);
                        let channel_item = ResponseChannelItem::Block(channel);
                        let peer = self.p2p_service.peer_manager().get_peer_id_with_height(&height);
                        let _ = self.p2p_service.send_request_msg(peer, request_msg, channel_item);
                    }
                    Some(TaskRequest::GetSealedHeader{ height, channel: response }) => {
                        let request_msg = RequestMessage::SealedHeader(height);
                        let channel_item = ResponseChannelItem::SealedHeader(response);
                        let peer = self.p2p_service.peer_manager().get_peer_id_with_height(&height);
                        let _ = self.p2p_service.send_request_msg(peer, request_msg, channel_item);
                    }
                    Some(TaskRequest::GetTransactions { block_id, from_peer, channel }) => {
                        let request_msg = RequestMessage::Transactions(block_id);
                        let channel_item = ResponseChannelItem::Transactions(channel);
                        let _ = self.p2p_service.send_request_msg(Some(from_peer), request_msg, channel_item);
                    }
                    Some(TaskRequest::RespondWithGossipsubMessageReport((message, acceptance))) => {
                        report_message(&mut self.p2p_service, message, acceptance);
                    }
                    Some(TaskRequest::RespondWithPeerReport { peer_id, score, reporting_service }) => {
                        self.p2p_service.report_peer(peer_id, score, reporting_service)
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

                        let _ = self.shared.block_height_broadcast.send(block_height_data);
                    }
                    Some(FuelP2PEvent::GossipsubMessage { message, message_id, peer_id,.. }) => {
                        let message_id = message_id.0;

                        match message {
                            GossipsubMessage::NewTx(transaction) => {
                                let next_transaction = GossipData::new(transaction, peer_id, message_id);
                                let _ = self.shared.tx_broadcast.send(next_transaction);
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
                                // TODO: Process `StorageError` somehow.
                                let block_response = self.db.get_sealed_block(&block_height)?
                                    .map(Arc::new);
                                let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::Block(block_response));
                            }
                            RequestMessage::Transactions(block_id) => {
                                let transactions_response = self.db.get_transactions(&block_id)?
                                    .map(Arc::new);

                                let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::Transactions(transactions_response));
                            }
                            RequestMessage::SealedHeader(block_height) => {
                                let response = self.db.get_sealed_header(&block_height)?
                                    .map(Arc::new);

                                let _ = self.p2p_service.send_response_msg(request_id, OutboundResponse::SealedHeader(response));
                            }
                        }
                    },
                    _ => (),
                }
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

    pub async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<(Vec<u8>, SealedBlockHeader)>> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(TaskRequest::GetSealedHeader {
                height,
                channel: sender,
            })
            .await?;

        receiver
            .await
            .map(|o| o.map(|(peer_id, header)| (peer_id.to_bytes(), header)))
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
    use crate::ports::P2pDb;

    use super::*;

    use fuel_core_services::Service;
    use fuel_core_storage::Result as StorageResult;
    use fuel_core_types::{
        blockchain::{
            block::Block,
            consensus::{
                poa::PoAConsensus,
                Consensus,
            },
        },
        fuel_types::BlockHeight,
    };

    #[derive(Clone, Debug)]
    struct FakeDb;

    impl P2pDb for FakeDb {
        fn get_sealed_block(
            &self,
            _height: &BlockHeight,
        ) -> StorageResult<Option<SealedBlock>> {
            let block = Block::new(Default::default(), vec![], &[]);

            Ok(Some(SealedBlock {
                entity: block,
                consensus: Consensus::PoA(PoAConsensus::new(Default::default())),
            }))
        }

        fn get_sealed_header(
            &self,
            _height: &BlockHeight,
        ) -> StorageResult<Option<SealedBlockHeader>> {
            let header = Default::default();

            Ok(Some(SealedBlockHeader {
                entity: header,
                consensus: Consensus::PoA(PoAConsensus::new(Default::default())),
            }))
        }

        fn get_transactions(
            &self,
            _block_id: &fuel_core_types::blockchain::primitives::BlockId,
        ) -> StorageResult<Option<Vec<Transaction>>> {
            Ok(Some(vec![]))
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
}
