use std::sync::Arc;

use anyhow::anyhow;

use fuel_core_interfaces::{
    model::SealedFuelBlock,
    p2p::{
        BlockBroadcast,
        BlockGossipData,
        ConsensusBroadcast,
        ConsensusGossipData,
        GossipData,
        GossipsubMessageAcceptance,
        GossipsubMessageInfo,
        P2pDb,
        P2pRequestEvent,
        TransactionBroadcast,
        TransactionGossipData,
    },
};
use libp2p::{
    gossipsub::MessageAcceptance,
    request_response::RequestId,
    PeerId,
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

    /// External P2P Request Events received from different Fuel Components
    rx_p2p_request: Receiver<P2pRequestEvent>,
    /// Internal Orchestrator Requests generated either from the enclosing `NetworkService`
    /// or from the `NetworkOrchestrator` itself
    rx_orchestrator_request: Receiver<OrchestratorRequest>,

    // Broadcasters
    // Broadcast different p2p events to external Fuel Components
    tx_consensus: Sender<ConsensusGossipData>,
    tx_transaction: broadcast::Sender<TransactionGossipData>,
    tx_block: Sender<BlockGossipData>,

    /// Generate internal Orchestrator Requests
    tx_orchestrator_request: Sender<OrchestratorRequest>,
}

enum OrchestratorRequest {
    Stop,
    GetPeersIds(oneshot::Sender<Vec<PeerId>>),
    RespondWithRequestedBlock((Option<Arc<SealedFuelBlock>>, RequestId)),
}

impl NetworkOrchestrator {
    fn new(
        p2p_config: P2PConfig,
        db: Arc<dyn P2pDb>,

        rx_p2p_request: Receiver<P2pRequestEvent>,
        orchestrator_request_channels: (
            Receiver<OrchestratorRequest>,
            Sender<OrchestratorRequest>,
        ),

        tx_consensus: Sender<ConsensusGossipData>,
        tx_transaction: broadcast::Sender<TransactionGossipData>,
        tx_block: Sender<BlockGossipData>,
    ) -> Self {
        let (rx_orchestrator_request, tx_orchestrator_request) =
            orchestrator_request_channels;

        Self {
            p2p_config,
            db,
            rx_p2p_request,
            rx_orchestrator_request,
            tx_block,
            tx_consensus,
            tx_transaction,
            tx_orchestrator_request,
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
                        Some(OrchestratorRequest::Stop) => break,
                        Some(OrchestratorRequest::GetPeersIds(channel)) => {
                            let _ = channel.send(p2p_service.get_peers_ids());
                        }
                        Some(OrchestratorRequest::RespondWithRequestedBlock((response, request_id))) => {
                            let _ = p2p_service.send_response_msg(request_id, response.map(OutboundResponse::ResponseBlock));
                        }
                        _ => {}
                    }
                }
                p2p_event = p2p_service.next_event() => {
                    match p2p_event {
                        Some(FuelP2PEvent::GossipsubMessage { message, message_id, peer_id,.. }) => {
                            let message_id = message_id.0;

                            match message {
                                GossipsubMessage::NewTx(tx) => {
                                    let _ = self.tx_transaction.send(GossipData::new(TransactionBroadcast::NewTransaction(tx), peer_id, message_id));
                                },
                                GossipsubMessage::NewBlock(block) => {
                                    let _ = self.tx_block.send(GossipData::new(BlockBroadcast::NewBlock(block), peer_id, message_id));
                                },
                                GossipsubMessage::ConsensusVote(vote) => {
                                    let _ = self.tx_consensus.send(GossipData::new(ConsensusBroadcast::NewVote(vote), peer_id, message_id));
                                },
                            }
                        },
                        Some(FuelP2PEvent::RequestMessage { request_message, request_id }) => {
                            match request_message {
                                RequestMessage::RequestBlock(block_height) => {
                                    let db = self.db.clone();
                                    let tx_orchestrator_request = self.tx_orchestrator_request.clone();

                                    tokio::spawn(async move {
                                        let block_response = db.get_sealed_block(block_height).await;
                                        let _ = tx_orchestrator_request.send(OrchestratorRequest::RespondWithRequestedBlock((block_response, request_id)));
                                    });
                                }
                            }
                        },
                        _ => {}
                    }
                },
                module_request_msg = self.rx_p2p_request.recv() => {
                    if let Some(request_event) = module_request_msg {
                        match request_event {
                            P2pRequestEvent::RequestBlock { height, response } => {
                                let request_msg = RequestMessage::RequestBlock(height);
                                let channel_item = ResponseChannelItem::ResponseBlock(response);
                                let _ = p2p_service.send_request_msg(None, request_msg, channel_item);
                            },
                            P2pRequestEvent::BroadcastNewBlock { block } => {
                                let broadcast = GossipsubBroadcastRequest::NewBlock(block);
                                let _ = p2p_service.publish_message(broadcast);
                            },
                            P2pRequestEvent::BroadcastNewTransaction { transaction } => {
                                let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                                let _ = p2p_service.publish_message(broadcast);
                            },
                            P2pRequestEvent::BroadcastConsensusVote { vote } => {
                                let broadcast = GossipsubBroadcastRequest::ConsensusVote(vote);
                                let _ = p2p_service.publish_message(broadcast);
                            },
                            P2pRequestEvent::GossipsubMessageReport { message, acceptance } => {
                                report_message(message, acceptance, &mut p2p_service);
                            }
                            P2pRequestEvent::Stop => break,
                        }
                    } else {
                        warn!(target: "fuel-libp2p", "Failed to receive P2PRequestEvent");
                    }
                }
            }
        }

        Ok(self)
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

pub struct Service {
    /// Network Orchestrator that handles p2p network and inter-module communication
    network_orchestrator: Arc<Mutex<Option<NetworkOrchestrator>>>,
    /// Holds the spawned task when Netowrk Orchestrator is started
    join: Mutex<Option<JoinHandle<Result<NetworkOrchestrator, anyhow::Error>>>>,
    /// Used for communicating with the Orchestrator
    tx_orchestrator_request: Sender<OrchestratorRequest>,
}

impl Service {
    pub fn new(
        p2p_config: P2PConfig,
        db: Arc<dyn P2pDb>,
        rx_p2p_request: Receiver<P2pRequestEvent>,
        tx_consensus: Sender<ConsensusGossipData>,
        tx_transaction: broadcast::Sender<TransactionGossipData>,
        tx_block: Sender<BlockGossipData>,
    ) -> Self {
        let (tx_orchestrator_request, rx_orchestrator_request) =
            tokio::sync::mpsc::channel(100);

        let network_orchestrator = NetworkOrchestrator::new(
            p2p_config,
            db,
            rx_p2p_request,
            (rx_orchestrator_request, tx_orchestrator_request.clone()),
            tx_consensus,
            tx_transaction,
            tx_block,
        );

        Self {
            join: Mutex::new(None),
            network_orchestrator: Arc::new(Mutex::new(Some(network_orchestrator))),
            tx_orchestrator_request,
        }
    }

    pub async fn get_peers_ids(&self) -> anyhow::Result<Vec<PeerId>> {
        let (sender, receiver) = oneshot::channel();

        let _ = self
            .tx_orchestrator_request
            .send(OrchestratorRequest::GetPeersIds(sender))
            .await;

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
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use async_trait::async_trait;
    use fuel_core_interfaces::model::{
        BlockHeight,
        FuelBlock,
        FuelBlockConsensus,
        FuelBlockPoAConsensus,
        SealedFuelBlock,
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
        ) -> Option<Arc<SealedFuelBlock>> {
            let block = FuelBlock::new(Default::default(), vec![], &[]);

            Some(Arc::new(SealedFuelBlock {
                block,
                consensus: FuelBlockConsensus::PoA(FuelBlockPoAConsensus::new(
                    Default::default(),
                )),
            }))
        }
    }

    #[tokio::test]
    async fn start_stop_works() {
        let p2p_config = P2PConfig::default_with_network("start_stop_works");
        let db: Arc<dyn P2pDb> = Arc::new(FakeDb);

        let (_, rx_request_event) = tokio::sync::mpsc::channel(100);
        let (tx_consensus, _) = tokio::sync::mpsc::channel(100);
        let (tx_transaction, _) = tokio::sync::broadcast::channel(100);
        let (tx_block, _) = tokio::sync::mpsc::channel(100);

        let service = Service::new(
            p2p_config,
            db.clone(),
            rx_request_event,
            tx_consensus,
            tx_transaction,
            tx_block,
        );

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
