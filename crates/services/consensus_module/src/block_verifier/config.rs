//! The config of the block verifier.

use fuel_core_chain_config::ConsensusConfig;
use fuel_core_types::{blockchain::primitives::DaBlockHeight, fuel_types::BlockHeight};

/// The config of the block verifier.
pub struct Config {
    /// The consensus config.
    pub consensus: ConsensusConfig,
    /// The block height of the genesis block.
    pub block_height: BlockHeight,
    /// The DA block height at genesis block.
    pub da_block_height: DaBlockHeight,
}

impl Config {
    /// Creates the verifier config for all possible consensuses.
    pub fn new(
        consensus: ConsensusConfig,
        block_height: BlockHeight,
        da_block_height: DaBlockHeight,
    ) -> Self {
        Self {
            consensus,
            block_height,
            da_block_height,
        }
    }
}
