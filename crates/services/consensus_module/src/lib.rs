//! Common traits and logic for managing the lifecycle of services
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

use core::time::Duration;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

pub mod block_verifier;

/// Config for settings the consensus needs that are related to the relayer.
#[derive(Clone, Debug)]
pub struct RelayerConsensusConfig {
    /// The maximum number of blocks that need to be synced before we start
    /// awaiting relayer syncing.
    pub max_da_lag: DaBlockHeight,
    /// The maximum time to wait for the relayer to sync.
    pub max_wait_time: Duration,
}

impl Default for RelayerConsensusConfig {
    fn default() -> Self {
        Self {
            max_da_lag: 10u64.into(),
            max_wait_time: Duration::from_secs(30),
        }
    }
}
