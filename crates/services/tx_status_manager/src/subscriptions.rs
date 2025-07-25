use fuel_core_services::stream::{BoxAsyncIter};
use crate::ports::P2PPreConfirmationGossipData;

pub(super) struct Subscriptions {
    pub new_tx_status: BoxAsyncIter<P2PPreConfirmationGossipData>,
}
