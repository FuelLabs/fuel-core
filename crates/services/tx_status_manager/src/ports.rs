use fuel_core_services::stream::BoxStream;
use fuel_core_types::services::p2p::{
    NetworkData,
    PreconfirmationMessage,
};

pub trait P2PSubscriptions {
    type GossipedStatuses: NetworkData<PreconfirmationMessage>;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses>;
}
