use fuel_core_services::stream::BoxStream;

use crate::ports::P2PPreConfirmationGossipData;

pub(super) struct Subscriptions {
    pub new_tx_status: BoxStream<P2PPreConfirmationGossipData>,
}
