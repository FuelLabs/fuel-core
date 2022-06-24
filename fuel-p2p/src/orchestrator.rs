use fuel_core_interfaces::p2p::{
    BlockBroadcast, ConsensusBroadcast, P2PRequestEvent, TransactionBroadcast,
};
use libp2p::identity::Keypair;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::warn;

use crate::{
    behavior::FuelBehaviourEvent,
    config::P2PConfig,
    gossipsub::messages::{GossipsubBroadcastRequest, GossipsubMessage},
    request_response::messages::{RequestMessage, ResponseChannelItem},
    service::{FuelP2PEvent, FuelP2PService},
};

pub struct NetworkOrchestrator {
    p2p_service: FuelP2PService,

    /// receives messages from different Fuel components
    rx_request_event: Receiver<P2PRequestEvent>,

    // senders
    tx_consensus: Sender<ConsensusBroadcast>,
    tx_transaction: Sender<TransactionBroadcast>,
    tx_block: Sender<BlockBroadcast>,
}

impl NetworkOrchestrator {
    pub async fn new(
        local_keypair: Keypair,
        p2p_config: P2PConfig,
        rx_request_event: Receiver<P2PRequestEvent>,

        tx_consensus: Sender<ConsensusBroadcast>,
        tx_transaction: Sender<TransactionBroadcast>,
        tx_block: Sender<BlockBroadcast>,
    ) -> Self {
        let p2p_service = FuelP2PService::new(local_keypair, p2p_config)
            .await
            .unwrap();

        Self {
            p2p_service,
            rx_request_event,
            tx_block,
            tx_consensus,
            tx_transaction,
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                p2p_event = self.p2p_service.next_event() => {
                    if let FuelP2PEvent::Behaviour(behaviour_event) = p2p_event {
                        match behaviour_event {
                            FuelBehaviourEvent::GossipsubMessage { message, .. } => {
                                match message {
                                    GossipsubMessage::NewTx(tx) => {
                                        let _ = self.tx_transaction.send(TransactionBroadcast::NewTransaction(tx));
                                    },
                                    GossipsubMessage::NewBlock(block) => {
                                        let _ = self.tx_block.send(BlockBroadcast::NewBlock(block));
                                    },
                                    GossipsubMessage::ConensusVote(vote) => {
                                        let _ = self.tx_consensus.send(ConsensusBroadcast::NewVote(vote));
                                    },
                                }
                            },
                            FuelBehaviourEvent::RequestMessage { request_message, request_id } => {
                                tokio::spawn(async move {

                                });
                            },
                            _ => {}
                        }
                    }
                },
                module_request_msg = self.rx_request_event.recv() => {
                    if let Some(request_event) = module_request_msg {
                        match request_event {
                            P2PRequestEvent::RequestBlock { height, response } => {
                                let request_msg = RequestMessage::RequestBlock(height);
                                let channel_item = ResponseChannelItem::ResponseBlock(response);
                                let _ = self.p2p_service.send_request_msg(None, request_msg, channel_item);
                            },
                            P2PRequestEvent::BroadcastNewBlock { block } => {
                                let broadcast = GossipsubBroadcastRequest::NewBlock(block);
                                let _ = self.p2p_service.publish_message(broadcast);
                            },
                            P2PRequestEvent::BroadcastNewTransaction { transaction } => {
                                let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                                let _ = self.p2p_service.publish_message(broadcast);
                            },
                            P2PRequestEvent::BroadcastConsensusVote { vote } => {
                                let broadcast = GossipsubBroadcastRequest::ConensusVote(vote);
                                let _ = self.p2p_service.publish_message(broadcast);
                            }
                        }
                    } else {
                        warn!(target: "fuel-libp2p", "Failed to receive P2PRequestEvent");
                    }
                }


            }
        }
    }
}
