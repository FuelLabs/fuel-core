use std::sync::Arc;

use fuel_core_types::fuel_tx::Transaction;

use fuel_core_types::blockchain::{
    block::Block,
    consensus::ConsensusVote,
};
use serde::{
    Deserialize,
    Serialize,
};

/// Used to inform `GossipsubCodec` to which GossipsubMessage decode to
/// GossipTopicTag is decided by checking received TopicHash from the peer
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GossipTopicTag {
    NewTx,
    NewBlock,
    ConsensusVote,
}

/// Takes Arc<T> and wraps it in a matching GossipsubBroadcastRequest
/// The inner referenced value is serialized and broadcast to the network
/// It is deserialized as `GossipsubMessage`
#[derive(Debug, Clone)]
pub enum GossipsubBroadcastRequest {
    NewTx(Arc<Transaction>),
    NewBlock(Arc<Block>),
    ConsensusVote(Arc<ConsensusVote>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GossipsubMessage {
    NewTx(Transaction),
    NewBlock(Block),
    ConsensusVote(ConsensusVote),
}
