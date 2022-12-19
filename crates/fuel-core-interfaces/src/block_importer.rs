use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::ConsensusVote,
        SealedBlock,
    },
    fuel_types::Bytes32,
};
use std::sync::Arc;

pub enum ImportBlockMpsc {
    ImportSealedBlock {
        block: Arc<SealedBlock>,
    },
    ImportFuelBlock {
        block: Arc<Block>,
    },
    SealFuelBlock {
        votes: Vec<ConsensusVote>,
        block_id: Bytes32,
    },
    Stop,
}
