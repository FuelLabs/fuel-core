use anyhow::anyhow;
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
use libp2p_gossipsub::{
    error::PublishError,
    MessageId,
};
use std::{
    fmt::Debug,
    sync::Arc,
};
use tokio::{
    sync::{
        broadcast,
        mpsc::{
            Receiver,
            Sender,
        },
        oneshot,
        Mutex,
    },
    task::JoinHandle,
};
use tracing::{
    debug,
    error,
    warn,
};

use crate::{
    codecs::{
        bincode::BincodeCodec,
        NetworkCodec,
    },
    config::P2PConfig,
    gossipsub::messages::{
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    ports::P2pDb,
    request_response::messages::{
        OutboundResponse,
        RequestMessage,
        ResponseChannelItem,
    },
    service::{
        FuelP2PEvent,
        FuelP2PService,
    },
};

/// Orchestrates various p2p-related events between the inner `P2pService`
/// and the top level `NetworkService`.
struct NetworkOrchestrator {
    p2p_config: P2PConfig,
    db: Arc<dyn P2pDb>,
    /// Receive internal Orchestrator Requests
    rx_orchestrator_request: Receiver<OrchestratorRequest>,
    /// Generate internal Orchestrator Requests
    tx_orchestrator_request: Sender<OrchestratorRequest>,
    tx_broadcast: Arc<broadcast::Sender<TransactionGossipData>>,
}

type BroadcastResponse = Result<MessageId, PublishError>;

enum OrchestratorRequest {
    // Broadcast requests to p2p network
    BroadcastTransaction((Arc<Transaction>, oneshot::Sender<BroadcastResponse>)),
    BroadcastBlock((Arc<Block>, oneshot::Sender<BroadcastResponse>)),
    BroadcastVote((Arc<ConsensusVote>, oneshot::Sender<BroadcastResponse>)),
    // Request to get one-off data from p2p network
    GetPeersIds(oneshot::Sender<Vec<PeerId>>),
    GetBlock((BlockHeight, oneshot::Sender<SealedBlock>)),
    // Responds back to the p2p network
    RespondWithGossipsubMessageReport((GossipsubMessageInfo, GossipsubMessageAcceptance)),
    RespondWithRequestedBlock((Option<Arc<SealedBlock>>, RequestId)),
    // Request to Stop the Network Orchestrator / Service
    Stop,
}

impl Debug for OrchestratorRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OrchestratorRequest")
    }
}

impl NetworkOrchestrator {
    fn new(
        p2p_config: P2PConfig,
        db: Arc<dyn P2pDb>,
        orchestrator_request_channels: (
            Receiver<OrchestratorRequest>,
            Sender<OrchestratorRequest>,
        ),
    ) -> Self {
        let (rx_orchestrator_request, tx_orchestrator_request) =
            orchestrator_request_channels;
        let (tx_broadcast, _) = broadcast::channel(100);

        Self {
            p2p_config,
            db,
            rx_orchestrator_request,
            tx_orchestrator_request,
            tx_broadcast: Arc::new(tx_broadcast),
        }
    }

    pub async fn run(mut self) -> anyhow::Result<Self> {
        let mut p2p_service = FuelP2PService::new(
            self.p2p_config.clone(),
            BincodeCodec::new(self.p2p_config.max_block_size),
        )?;

        loop {
            tokio::select! {
                next_service_request = self.rx_orchestrator_request.recv() => {
                    match next_service_request {
                        Some(OrchestratorRequest::BroadcastTransaction((transaction, sender))) => {
                            let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                            let _ = sender.send(p2p_service.publish_message(broadcast));
                        }
                        Some(OrchestratorRequest::BroadcastBlock((block, sender))) => {
                            let broadcast = GossipsubBroadcastRequest::NewBlock(block);
                            let _ = sender.send(p2p_service.publish_message(broadcast));
                        }
                        Some(OrchestratorRequest::BroadcastVote((vote, sender))) => {
                            let broadcast = GossipsubBroadcastRequest::ConsensusVote(vote);
                            let _ = sender.send(p2p_service.publish_message(broadcast));
                        }
                        Some(OrchestratorRequest::GetPeersIds(channel)) => {
                            let _ = channel.send(p2p_service.get_peers_ids());
                        }
                        Some(OrchestratorRequest::GetBlock((height, response))) => {
                            let request_msg = RequestMessage::RequestBlock(height);
                            let channel_item = ResponseChannelItem::ResponseBlock(response);
                            let _ = p2p_service.send_request_msg(None, request_msg, channel_item);
                        }
                        Some(OrchestratorRequest::RespondWithGossipsubMessageReport((message, acceptance))) => {
                            report_message(message, acceptance, &mut p2p_service);
                        }
                        Some(OrchestratorRequest::RespondWithRequestedBlock((response, request_id))) => {
                            let _ = p2p_service.send_response_msg(request_id, response.map(OutboundResponse::ResponseBlock));
                        }
                        Some(OrchestratorRequest::Stop) => break,
                        None => {}
                    }
                }
                p2p_event = p2p_service.next_event() => {
                    match p2p_event {
                        Some(FuelP2PEvent::GossipsubMessage { message, message_id, peer_id,.. }) => {
                            let message_id = message_id.0;

                            match message {
                                GossipsubMessage::NewTx(transaction) => {
                                    let next_transaction = GossipData::new(transaction, peer_id, message_id);
                                    let _ = self.tx_broadcast.send(next_transaction);
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
                                    let tx_orchestrator_request = self.tx_orchestrator_request.clone();

                                    tokio::spawn(async move {
                                        // TODO: Process `StorageError` somehow.
                                        let block_response = db.get_sealed_block(block_height)
                                            .await
                                            .expect("Didn't expect error from database")
                                            .map(Arc::new);
                                        let _ = tx_orchestrator_request.send(OrchestratorRequest::RespondWithRequestedBlock((block_response, request_id)));
                                    });
                                }
                            }
                        },
                        _ => {}
                    }
                },
            }
        }

        Ok(self)
    }

    pub fn sender(&self) -> Arc<broadcast::Sender<TransactionGossipData>> {
        self.tx_broadcast.clone()
    }
}

pub struct Service {
    /// Network Orchestrator that handles p2p network and inter-module communication
    network_orchestrator: Arc<Mutex<Option<NetworkOrchestrator>>>,
    /// Holds the spawned task when Netowrk Orchestrator is started
    join: Mutex<Option<JoinHandle<Result<NetworkOrchestrator, anyhow::Error>>>>,
    /// Used for communicating with the Orchestrator
    tx_orchestrator_request: Sender<OrchestratorRequest>,
    /// Sender of p2p transaction used for subscribing.
    tx_broadcast: Arc<broadcast::Sender<TransactionGossipData>>,
}

impl Service {
    pub fn new(p2p_config: P2PConfig, db: Arc<dyn P2pDb>) -> Self {
        let (tx_orchestrator_request, rx_orchestrator_request) =
            tokio::sync::mpsc::channel(100);

        let network_orchestrator = NetworkOrchestrator::new(
            p2p_config,
            db,
            (rx_orchestrator_request, tx_orchestrator_request.clone()),
        );
        let tx_broadcast = network_orchestrator.sender();

        Self {
            join: Mutex::new(None),
            network_orchestrator: Arc::new(Mutex::new(Some(network_orchestrator))),
            tx_orchestrator_request,
            tx_broadcast,
        }
    }

    pub async fn notify_gossip_transaction_validity<'a, T>(
        &self,
        message: &'a T,
        acceptance: GossipsubMessageAcceptance,
    ) where
        GossipsubMessageInfo: From<&'a T>,
    {
        let msg_info = message.into();

        let _ = self
            .tx_orchestrator_request
            .send(OrchestratorRequest::RespondWithGossipsubMessageReport((
                msg_info, acceptance,
            )))
            .await;
    }

    pub async fn get_block(&self, height: BlockHeight) -> anyhow::Result<SealedBlock> {
        let (sender, receiver) = oneshot::channel();

        self.tx_orchestrator_request
            .send(OrchestratorRequest::GetBlock((height, sender)))
            .await?;

        receiver.await.map_err(|e| anyhow!("{}", e))
    }

    pub async fn broadcast_vote(&self, vote: Arc<ConsensusVote>) -> anyhow::Result<()> {
        let (sender, receiver) = oneshot::channel();

        self.tx_orchestrator_request
            .send(OrchestratorRequest::BroadcastVote((vote, sender)))
            .await?;

        receiver.await?.map(|_| ()).map_err(|e| anyhow!("{}", e))
    }

    pub async fn broadcast_block(&self, block: Arc<Block>) -> anyhow::Result<()> {
        let (sender, receiver) = oneshot::channel();

        self.tx_orchestrator_request
            .send(OrchestratorRequest::BroadcastBlock((block, sender)))
            .await?;

        receiver.await?.map(|_| ()).map_err(|e| anyhow!("{}", e))
    }

    pub async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        let (sender, receiver) = oneshot::channel();

        self.tx_orchestrator_request
            .send(OrchestratorRequest::BroadcastTransaction((
                transaction,
                sender,
            )))
            .await?;

        receiver.await?.map(|_| ()).map_err(|e| anyhow!("{}", e))
    }

    pub async fn get_peers_ids(&self) -> anyhow::Result<Vec<PeerId>> {
        let (sender, receiver) = oneshot::channel();

        self.tx_orchestrator_request
            .send(OrchestratorRequest::GetPeersIds(sender))
            .await?;

        receiver.await.map_err(|e| anyhow!("{}", e))
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let mut join = self.join.lock().await;

        if join.is_none() {
            if let Some(network_orchestrator) =
                self.network_orchestrator.lock().await.take()
            {
                *join = Some(tokio::spawn(async { network_orchestrator.run().await }));

                Ok(())
            } else {
                Err(anyhow!("Starting Network Orchestrator that is stopping"))
            }
        } else {
            Err(anyhow!("Network Orchestrator already started"))
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let join_handle = self.join.lock().await.take();

        if let Some(join_handle) = join_handle {
            let network_orchestrator = self.network_orchestrator.clone();
            let _ = self
                .tx_orchestrator_request
                .send(OrchestratorRequest::Stop)
                .await;
            Some(tokio::spawn(async move {
                if let Ok(res) = join_handle.await {
                    *network_orchestrator.lock().await = res.ok();
                }
            }))
        } else {
            None
        }
    }

    pub fn subscribe_tx(&self) -> broadcast::Receiver<TransactionGossipData> {
        self.tx_broadcast.subscribe()
    }
}

fn report_message<T: NetworkCodec>(
    message: GossipsubMessageInfo,
    acceptance: GossipsubMessageAcceptance,
    p2p_service: &mut FuelP2PService<T>,
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

    use fuel_core_storage::Result as StorageResult;
    use fuel_core_types::blockchain::{
        block::Block,
        consensus::{
            poa::PoAConsensus,
            Consensus,
        },
        primitives::BlockHeight,
    };
    use tokio::time::{
        sleep,
        Duration,
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
    async fn start_stop_works() {
        let p2p_config = P2PConfig::default_initialized("start_stop_works");
        let db: Arc<dyn P2pDb> = Arc::new(FakeDb);

        let service = Service::new(p2p_config, db.clone());

        // Node with p2p service started
        assert!(service.start().await.is_ok());
        sleep(Duration::from_secs(1)).await;
        // Node with p2p service stopped
        assert!(service.stop().await.is_some());
        sleep(Duration::from_secs(1)).await;

        // Node with p2p service successfully restarted
        assert!(service.start().await.is_ok());
    }
}
