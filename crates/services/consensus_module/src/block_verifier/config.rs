//! The config of the block verifier.

use fuel_core_chain_config::ChainConfig;

/// The config of the block verifier.
pub struct Config {
    /// The chain configuration.
    pub chain_config: ChainConfig,
}

impl Config {
    /// Creates the verifier config for all possible consensuses.
    pub fn new(chain_config: ChainConfig) -> Self {
        Self { chain_config }
    }
}
