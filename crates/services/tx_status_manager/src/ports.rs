use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    fuel_tx::Bytes64,
    services::p2p::{
        DelegatePublicKey,
        GossipData,
        GossipsubMessageAcceptance,
        GossipsubMessageInfo,
        NetworkData,
        PreConfirmationMessage,
        ProtocolSignature,
    },
};

pub type P2PPreConfirmationMessage =
    PreConfirmationMessage<DelegatePublicKey, Bytes64, ProtocolSignature>;

pub type P2PPreConfirmationGossipData = GossipData<P2PPreConfirmationMessage>;

pub trait P2PSubscriptions: Send {
    type GossipedStatuses: NetworkData<P2PPreConfirmationMessage>;

    fn gossiped_tx_statuses(&self) -> BoxStream<Self::GossipedStatuses>;

    /// Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;
}
