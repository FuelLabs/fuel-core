//! The config of the block verifier.

use fuel_core_chain_config::{
    BlockProduction,
    ChainConfig,
};
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
        let BlockProduction::ProofOfAuthority { trigger } =
            chain_config.block_production.clone();
        Self {
            chain_config,
            poa: PoAVerifierConfig {
                enabled_manual_blocks,
                // TODO: Make it `true`. See the comment of the `perform_strict_time_rules`.
                perform_strict_time_rules: false,
                production: trigger,
            },
        }
    }
}
