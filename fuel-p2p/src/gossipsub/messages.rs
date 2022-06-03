use fuel_core_interfaces::model::{FuelBlock, Vote};
use fuel_tx::Transaction;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GossipsubMessage {
    BroadcastNewTx(Transaction),
    BroadcastNewBlock(FuelBlock),
    BroadcastConensusVote(Vote),
}
