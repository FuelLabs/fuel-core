//! The config of the block verifier.

use fuel_core_chain_config::ChainConfig;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};

/// The config of the block verifier.
pub struct Config {
    /// The chain configuration.
    pub chain_config: ChainConfig,
    /// The block height of the genesis block.
    pub block_height: BlockHeight,
    /// The DA block height at genesis block.
    pub da_block_height: DaBlockHeight,
}

impl Config {
    /// Creates the verifier config for all possible consensuses.
    pub fn new(
        chain_config: ChainConfig,
        block_height: BlockHeight,
        da_block_height: DaBlockHeight,
    ) -> Self {
        Self {
            chain_config,
            block_height,
            da_block_height,
        }
    }
}
