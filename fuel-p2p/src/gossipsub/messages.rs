use fuel_core_interfaces::model::FuelBlock;
use fuel_tx::Transaction;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum GossipsubMessage {
    BroadcastNewTx(Transaction),
    BroadcastNewBlock(FuelBlock),
    BroadcastConensusVote(Vote),
}
