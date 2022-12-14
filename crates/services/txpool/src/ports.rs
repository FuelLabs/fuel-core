use std::sync::Arc;

use fuel_core_interfaces::{
    common::prelude::Transaction,
    p2p::NetworkData,
};

#[async_trait::async_trait]
pub trait PeerToPeer: Send + Sync {
    type GossipedTransaction: NetworkData<Transaction>;
    // Gossip broadcast a transaction inserted via API.
    async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()>;
    // Await the next transaction from network gossip (similar to stream.next()).
    async fn next_gossiped_transaction(&self) -> Self::GossipedTransaction;
    // Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message: &Self::GossipedTransaction,
        validity: GossipValidity,
    );
}

pub enum GossipValidity {
    // Report whether the gossiped message is valid and safe to rebroadcast
    Accept,
    // Ignore the received message and prevent further gossiping
    Ignore,
    // Punish the gossip sender for providing invalid
    // (or malicious) data and prevent further gossiping
    Invalid,
}
