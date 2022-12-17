use fuel_core_types::{
    fuel_tx::Transaction,
    services::p2p::{
        GossipsubMessageAcceptance,
        NetworkData,
    },
};
use std::sync::Arc;

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
        validity: GossipsubMessageAcceptance,
    );
}
