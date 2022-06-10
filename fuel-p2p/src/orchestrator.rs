use fuel_core_interfaces::model::Vote;
use fuel_tx::Transaction;
use libp2p::identity::Keypair;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    behavior::FuelBehaviourEvent,
    config::P2PConfig,
    gossipsub::messages::GossipsubMessage,
    service::{FuelP2PEvent, FuelP2PService},
};

pub struct NetworkOrchestrator {
    p2p_service: FuelP2PService,

    // receivers
    rx_block_producer: Receiver<()>,
    rx_bft: Receiver<Vote>,

    // senders
    tx_synchronizer: Sender<()>,
    tx_transaction_pool: Sender<Transaction>,
    tx_bft: Sender<()>,
}

impl NetworkOrchestrator {
    pub async fn new(
        local_keypair: Keypair,
        p2p_config: P2PConfig,
        rx_block_producer: Receiver<()>,
        rx_bft: Receiver<Vote>,

        tx_synchronizer: Sender<()>,
        tx_transaction_pool: Sender<Transaction>,
        tx_bft: Sender<()>,
    ) -> Self {
        let p2p_service = FuelP2PService::new(local_keypair, p2p_config)
            .await
            .unwrap();

        Self {
            p2p_service,

            rx_bft,
            rx_block_producer,

            tx_bft,
            tx_synchronizer,
            tx_transaction_pool,
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                p2p_event = self.p2p_service.next_event() => {
                    if let FuelP2PEvent::Behaviour(behaviour_event) = p2p_event {
                        match behaviour_event {
                            FuelBehaviourEvent::GossipsubMessage { message, topic_hash, peer_id } => {
                                match message {
                                    GossipsubMessage::NewTx(tx) => {
                                        let _ = self.tx_transaction_pool.send(tx);
                                    },
                                    GossipsubMessage::NewBlock(block) => {

                                    },
                                    GossipsubMessage::ConensusVote(vote) => {

                                    },
                                }
                            },
                            FuelBehaviourEvent::RequestMessage { request_message, request_id } => {
                                tokio::spawn(async move {
                                    //let block = todo!();
                                    //let response_message = ResponseMessage::ResponseBlock(block);
                                    //let _ = self.p2p_service.send_response_msg(request_id, response_message);
                                });
                            },
                            _ => {}
                        }
                    }

                },
                bft_vote = self.rx_bft.recv() => {
                    if let Some(vote) = bft_vote {
                        // vote
                    }
                },

                block_producer_msg = self.rx_block_producer.recv() => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let _ = self.p2p_service.send_request_msg(None, todo!(), tx);
                }

            }
        }
    }
}
