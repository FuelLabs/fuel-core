//! The config of the block verifier.

use std::time::Duration;

use fuel_core_chain_config::ChainConfig;
use fuel_core_poa::verifier::Config as PoAVerifierConfig;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

/// The config of the block verifier.
pub struct Config {
    /// The chain configuration.
    pub chain_config: ChainConfig,
    /// The config of verifier for the PoA.
    pub poa: PoAVerifierConfig,
    /// Config for settings the verifier needs that are related to the relayer.
    pub relayer: RelayerVerifierConfig,
}

/// Config for settings the verifier needs that are related to the relayer.
#[derive(Clone, Debug)]
pub struct RelayerVerifierConfig {
    /// The maximum number of blocks that need to be synced before we start
    /// awaiting relayer syncing.
    pub max_da_lag: DaBlockHeight,
    /// The maximum time to wait for the relayer to sync.
    pub max_wait_time: Duration,
}

impl Default for RelayerVerifierConfig {
    fn default() -> Self {
        Self {
            max_da_lag: 10u64.into(),
            max_wait_time: Duration::from_secs(30),
        }
    }
}

impl Config {
    /// Creates the verifier config for all possible consensuses.
    pub fn new(
        chain_config: ChainConfig,
        enabled_manual_blocks: bool,
        relayer: RelayerVerifierConfig,
    ) -> Self {
        Self {
            chain_config,
            poa: PoAVerifierConfig {
                enabled_manual_blocks,
            },
            relayer,
        }
    }
}
