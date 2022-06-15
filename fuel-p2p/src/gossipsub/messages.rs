use fuel_core_interfaces::common::fuel_tx::Transaction;
use fuel_core_interfaces::model::{FuelBlock, Vote};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GossipsubMessage {
    NewTx(Transaction),
    NewBlock(FuelBlock),
    ConensusVote(Vote),
}
