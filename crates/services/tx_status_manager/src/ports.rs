use fuel_core_services::stream::BoxStream;
use fuel_core_types::{fuel_tx::TxId, services::{
    p2p::NetworkData,
    txpool::TransactionStatus,
}};

pub trait P2PSubscriptions {
    type GossipedStatuses: NetworkData<(TxId, TransactionStatus)>;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses>;
}
