use crate::{
    codecs::{
        bincode::BincodeCodec,
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
    ports::P2pDb,
    request_response::messages::{
        OutboundResponse,
        RequestMessage,
        ResponseChannelItem,
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::ConsensusVote,
        primitives::BlockHeight,
        SealedBlock,
    },
    fuel_tx::Transaction,
    services::p2p::{
        GossipData,
        GossipsubMessageAcceptance,
        TransactionGossipData,
    },
};
use libp2p::{
    gossipsub::MessageAcceptance,
    request_response::RequestId,
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
use tracing::{
    debug,
    error,
    warn,
};

pub type Service<D> = ServiceRunner<Task<D>>;

enum TaskRequest {
    // Broadcast requests to p2p network
    BroadcastTransaction(Arc<Transaction>),
    BroadcastBlock(Arc<Block>),
    BroadcastVote(Arc<ConsensusVote>),
    // Request to get one-off data from p2p network
    GetPeerIds(oneshot::Sender<Vec<PeerId>>),
    GetBlock((BlockHeight, oneshot::Sender<SealedBlock>)),
    // Responds back to the p2p network
    RespondWithGossipsubMessageReport((GossipsubMessageInfo, GossipsubMessageAcceptance)),
    RespondWithRequestedBlock((Option<Arc<SealedBlock>>, RequestId)),
}

impl Debug for TaskRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TaskRequest")
    }
}

/// Orchestrates various p2p-related events between the inner `P2pService`
/// and the top level `NetworkService`.
pub struct Task<D> {
    p2p_service: FuelP2PService<BincodeCodec>,
    db: Arc<D>,
    /// Receive internal Task Requests
    request_receiver: mpsc::Receiver<TaskRequest>,
    shared: SharedState,
}

impl<D> Task<D> {
    pub fn new(config: Config, db: Arc<D>) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(100);
        let (tx_broadcast, _) = broadcast::channel(100);
        let max_block_size = config.max_block_size;
        let p2p_service = FuelP2PService::new(config, BincodeCodec::new(max_block_size));

        Self {
            p2p_service,
            db,
            request_receiver,
            shared: SharedState {
                request_sender,
                tx_broadcast,
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

    fn shared_data(&self) -> Self::SharedData {
        self.shared.clone()
    }

    async fn into_task(mut self, _: &StateWatcher) -> anyhow::Result<Self::Task> {
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
        tokio::select! {
            biased;

            _ = watcher.while_started() => {}

            next_service_request = self.request_receiver.recv() => {
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
                        let _ = channel.send(self.p2p_service.get_peers_ids());
                    }
                    Some(TaskRequest::GetBlock((height, response))) => {
                        let request_msg = RequestMessage::RequestBlock(height);
                        let channel_item = ResponseChannelItem::ResponseBlock(response);
                        let _ = self.p2p_service.send_request_msg(None, request_msg, channel_item);
                    }
                    Some(TaskRequest::RespondWithGossipsubMessageReport((message, acceptance))) => {
                        report_message(&mut self.p2p_service, message, acceptance);
                    }
                    Some(TaskRequest::RespondWithRequestedBlock((response, request_id))) => {
                        let _ = self.p2p_service.send_response_msg(request_id, response.map(OutboundResponse::ResponseBlock));
                    }
                    None => {
                        unreachable!("The `Task` is holder of the `Sender`, so it should not be possible");
                    }
                }
            }
            p2p_event = self.p2p_service.next_event() => {
                match p2p_event {
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
                            RequestMessage::RequestBlock(block_height) => {
                                let db = self.db.clone();
                                let request_sender = self.shared.request_sender.clone();

                                tokio::spawn(async move {
                                    // TODO: Process `StorageError` somehow.
                                    let block_response = db.get_sealed_block(block_height)
                                        .await
                                        .expect("Didn't expect error from database")
                                        .map(Arc::new);
                                    let _ = request_sender.send(
                                        TaskRequest::RespondWithRequestedBlock(
                                            (block_response, request_id)
                                        )
                                    );
                                });
                            }
                        }
                    },
                    _ => {}
                }
            },
        }

        Ok(true /* should_continue */)
    }
}

#[derive(Clone)]
pub struct SharedState {
    /// Sender of p2p transaction used for subscribing.
    tx_broadcast: broadcast::Sender<TransactionGossipData>,
    /// Used for communicating with the `Task`.
    request_sender: mpsc::Sender<TaskRequest>,
}

impl SharedState {
    pub fn notify_gossip_transaction_validity<'a, T>(
        &self,
        message: &'a T,
        acceptance: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>
    where
        GossipsubMessageInfo: From<&'a T>,
    {
        let msg_info = message.into();

        self.request_sender
            .try_send(TaskRequest::RespondWithGossipsubMessageReport((
                msg_info, acceptance,
            )))?;
        Ok(())
    }

    pub async fn get_block(&self, height: BlockHeight) -> anyhow::Result<SealedBlock> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(TaskRequest::GetBlock((height, sender)))
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
}

pub fn new_service<D>(p2p_config: Config, db: D) -> Service<D>
where
    D: P2pDb + 'static,
{
    Service::new(Task::new(p2p_config, Arc::new(db)))
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

    if let Ok(peer_id) = peer_id.try_into() {
        let acceptance = match acceptance {
            GossipsubMessageAcceptance::Accept => MessageAcceptance::Accept,
            GossipsubMessageAcceptance::Reject => MessageAcceptance::Reject,
            GossipsubMessageAcceptance::Ignore => MessageAcceptance::Ignore,
        };

        match p2p_service.report_message_validation_result(&msg_id, &peer_id, acceptance)
        {
            Ok(true) => {
                debug!(target: "fuel-libp2p", "Sent a report for MessageId: {} from PeerId: {}", msg_id, peer_id);
            }
            Ok(false) => {
                warn!(target: "fuel-libp2p", "Message with MessageId: {} not found in the Gossipsub Message Cache", msg_id);
            }
            Err(e) => {
                error!(target: "fuel-libp2p", "Failed to publish Message with MessageId: {} with Error: {:?}", msg_id, e);
            }
        }
    } else {
        warn!(target: "fuel-libp2p", "Failed to read PeerId from received GossipsubMessageId: {}", msg_id);
    }
}

/// Lightweight representation of gossipped data that only includes IDs
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GossipsubMessageInfo {
    /// The message id that corresponds to a message payload (typically a unique hash)
    pub message_id: Vec<u8>,
    /// The ID of the network peer that sent this message
    pub peer_id: Vec<u8>,
}

impl<T> From<&GossipData<T>> for GossipsubMessageInfo {
    fn from(gossip_data: &GossipData<T>) -> Self {
        Self {
            message_id: gossip_data.message_id.clone(),
            peer_id: gossip_data.peer_id.clone(),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::ports::P2pDb;

    use super::*;
    use async_trait::async_trait;

    use fuel_core_services::Service;
    use fuel_core_storage::Result as StorageResult;
    use fuel_core_types::blockchain::{
        block::Block,
        consensus::{
            poa::PoAConsensus,
            Consensus,
        },
        primitives::BlockHeight,
    };

    #[derive(Clone, Debug)]
    struct FakeDb;

    #[async_trait]
    impl P2pDb for FakeDb {
        async fn get_sealed_block(
            &self,
            _height: BlockHeight,
        ) -> StorageResult<Option<SealedBlock>> {
            let block = Block::new(Default::default(), vec![], &[]);

            Ok(Some(SealedBlock {
                entity: block,
                consensus: Consensus::PoA(PoAConsensus::new(Default::default())),
            }))
        }
    }

    #[tokio::test]
    async fn start_and_stop_awaits_works() {
        let p2p_config = Config::default_initialized("start_stop_works");
        let service = new_service(p2p_config, FakeDb);

        // Node with p2p service started
        assert!(service.start_and_await().await.unwrap().started());
        // Node with p2p service stopped
        assert!(service.stop_and_await().await.unwrap().stopped());
    }
}
