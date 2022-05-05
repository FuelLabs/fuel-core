use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum GossipsubMessage {
    BroadcastNewTx,
    BroadcastNewBlock,
}
