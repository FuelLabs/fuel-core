//! The config of the block verifier.

use fuel_core_chain_config::ChainConfig;
use fuel_core_poa::verifier::Config as PoAVerifierConfig;

/// The config of the block verifier.
pub struct Config {
    /// The chain configuration.
    pub chain_config: ChainConfig,
    /// The config of verifier for the PoA.
    pub poa: PoAVerifierConfig,
}

impl Config {
    /// Creates the verifier config for all possible consensuses.
    pub fn new(chain_config: ChainConfig, enabled_manual_blocks: bool) -> Self {
        Self {
            chain_config,
            poa: PoAVerifierConfig {
                enabled_manual_blocks,
            },
        }
    }
}
