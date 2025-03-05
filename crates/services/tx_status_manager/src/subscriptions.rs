use fuel_core_services::stream::BoxStream;
use fuel_core_types::services::p2p::PreconfirmationsGossipData;

pub(super) struct Subscriptions {
    pub new_tx_status: BoxStream<PreconfirmationsGossipData>,
}
