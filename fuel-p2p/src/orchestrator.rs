use std::error::Error;
use std::sync::Arc;
use std::{future::Future, pin::Pin};

use fuel_core_interfaces::{
    p2p::{BlockBroadcast, ConsensusBroadcast, P2pRequestEvent, TransactionBroadcast},
    relayer::RelayerDb,
};
use futures::{stream::futures_unordered::FuturesUnordered, StreamExt};
use libp2p::identity::Keypair;
use libp2p::request_response::RequestId;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::warn;

use crate::{
    behavior::FuelBehaviourEvent,
    config::P2PConfig,
    gossipsub::messages::{GossipsubBroadcastRequest, GossipsubMessage},
    request_response::messages::{OutboundResponse, RequestMessage, ResponseChannelItem},
    service::{FuelP2PEvent, FuelP2PService},
};

type ResponseFuture = Pin<Box<dyn Future<Output = Option<(OutboundResponse, RequestId)>>>>;

pub struct NetworkOrchestrator {
    p2p_service: FuelP2PService,

    /// receives messages from different Fuel components
    rx_request_event: Receiver<P2pRequestEvent>,

    // senders
    tx_consensus: Sender<ConsensusBroadcast>,
    tx_transaction: Sender<TransactionBroadcast>,
    tx_block: Sender<BlockBroadcast>,

    db: Arc<dyn RelayerDb>,

    outbound_responses: FuturesUnordered<ResponseFuture>,
}

impl NetworkOrchestrator {
    pub async fn new(
        local_keypair: Keypair,
        p2p_config: P2PConfig,
        rx_request_event: Receiver<P2pRequestEvent>,

        tx_consensus: Sender<ConsensusBroadcast>,
        tx_transaction: Sender<TransactionBroadcast>,
        tx_block: Sender<BlockBroadcast>,

        db: Arc<dyn RelayerDb>,
    ) -> Result<Self, Box<dyn Error>> {
        let p2p_service = FuelP2PService::new(local_keypair, p2p_config).await?;

        Ok(Self {
            p2p_service,
            rx_request_event,
            tx_block,
            tx_consensus,
            tx_transaction,
            db,
            outbound_responses: Default::default(),
        })
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                next_response = self.outbound_responses.next() => {
                    if let Some(Some((response, request_id))) = next_response {
                        let _ = self.p2p_service.send_response_msg(request_id, response);
                    }
                },
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
                                    GossipsubMessage::ConsensusVote(vote) => {
                                        let _ = self.tx_consensus.send(ConsensusBroadcast::NewVote(vote));
                                    },
                                }
                            },
                            FuelBehaviourEvent::RequestMessage { request_message, request_id } => {
                                match request_message {
                                    RequestMessage::RequestBlock(block_height) => {
                                        let db = self.db.clone();

                                        self.outbound_responses.push(
                                            Box::pin(async move {
                                                db.get_sealed_block(block_height).await.map(|block| (OutboundResponse::ResponseBlock(block), request_id))
                                            })
                                        );
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                },
                module_request_msg = self.rx_request_event.recv() => {
                    if let Some(request_event) = module_request_msg {
                        match request_event {
                            P2pRequestEvent::RequestBlock { height, response } => {
                                let request_msg = RequestMessage::RequestBlock(height);
                                let channel_item = ResponseChannelItem::ResponseBlock(response);
                                let _ = self.p2p_service.send_request_msg(None, request_msg, channel_item);
                            },
                            P2pRequestEvent::BroadcastNewBlock { block } => {
                                let broadcast = GossipsubBroadcastRequest::NewBlock(block);
                                let _ = self.p2p_service.publish_message(broadcast);
                            },
                            P2pRequestEvent::BroadcastNewTransaction { transaction } => {
                                let broadcast = GossipsubBroadcastRequest::NewTx(transaction);
                                let _ = self.p2p_service.publish_message(broadcast);
                            },
                            P2pRequestEvent::BroadcastConsensusVote { vote } => {
                                let broadcast = GossipsubBroadcastRequest::ConsensusVote(vote);
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
