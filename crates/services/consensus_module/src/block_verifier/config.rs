//! The config of the block verifier.

use std::time::Duration;

use fuel_core_chain_config::ChainConfig;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

/// The config of the block verifier.
pub struct Config {
    /// The chain configuration.
    pub chain_config: ChainConfig,
    /// Config for settings the verifier needs that are related to the Relayer.
    pub relayer: RelayerVerifierConfig,
}

/// Config for settings the verifier needs that are related to the Relayer.
#[derive(Clone, Debug)]
pub struct RelayerVerifierConfig {
    /// The maximum number of blocks that need to be synchronized before we start
    /// awaiting Relayer syncing.
    pub max_da_lag: DaBlockHeight,
    /// The maximum time to wait for the Relayer to sync.
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
    pub fn new(chain_config: ChainConfig, relayer: RelayerVerifierConfig) -> Self {
        Self {
            chain_config,
            relayer,
        }
    }
}
