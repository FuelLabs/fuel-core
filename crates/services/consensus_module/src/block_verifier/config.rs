//! The config of the block verifier.

use fuel_core_chain_config::ChainConfig;
use fuel_core_types::fuel_types::BlockHeight;

/// The config of the block verifier.
pub struct Config {
    /// The chain configuration.
    pub chain_config: ChainConfig,
    // TODO: Decide do we need it here or inside of the `ChainConfig` or somewhere else.
    //  https://github.com/FuelLabs/fuel-core/issues/1667
    /// The block height at genesis
    pub block_height: BlockHeight,
}

impl Config {
    /// Creates the verifier config for all possible consensuses.
    pub fn new(chain_config: ChainConfig, block_height: BlockHeight) -> Self {
        Self {
            chain_config,
            block_height,
        }
    }
}
