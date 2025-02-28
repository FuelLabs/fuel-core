use fuel_core_services::stream::BoxStream;
use fuel_core_types::services::{
    p2p::NetworkData,
    txpool::TransactionStatus,
};

pub trait P2PSubscriptions {
    type GossipedStatuses: NetworkData<TransactionStatus>;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses>;
}
