use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub enum GossipsubMessage {
    BroadcastNewTx,
    BroadcastNewBlock,
}
