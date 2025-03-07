use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    fuel_tx::Bytes64,
    services::p2p::{
        DelegatePublicKey,
        GossipData,
        NetworkData,
        PreConfirmationMessage,
        ProtocolSignature,
    },
};

pub type P2PPreConfirmationMessage =
    PreConfirmationMessage<DelegatePublicKey, Bytes64, ProtocolSignature>;

pub type P2PPreConfirmationGossipData = GossipData<P2PPreConfirmationMessage>;

pub trait P2PSubscriptions {
    type GossipedStatuses: NetworkData<P2PPreConfirmationMessage>;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses>;
}
