use crate::ports::P2PPreConfirmationGossipData;
use fuel_core_services::stream::BoxStream;

pub(super) struct Subscriptions {
    pub new_tx_status: BoxStream<P2PPreConfirmationGossipData>,
}
