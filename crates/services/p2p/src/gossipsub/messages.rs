use crate::ports::P2PPreConfirmationMessage;
use fuel_core_types::fuel_tx::Transaction;
use serde::{
    Deserialize,
    Serialize,
};
use std::sync::Arc;

/// Used to inform `GossipsubCodec` to which GossipsubMessage decode to
/// GossipTopicTag is decided by checking received TopicHash from the peer
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GossipTopicTag {
    NewTx,
    TxPreconfirmations,
}

/// Takes `Arc<T>` and wraps it in a matching GossipsubBroadcastRequest
/// The inner referenced value is serialized and broadcast to the network
/// It is deserialized as `GossipsubMessage`
#[derive(Debug, Clone)]
pub enum GossipsubBroadcastRequest {
    NewTx(Arc<Transaction>),
    TxPreConfirmations(Arc<P2PPreConfirmationMessage>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GossipsubMessage {
    NewTx(Transaction),
    TxPreConfirmations(P2PPreConfirmationMessage),
}
