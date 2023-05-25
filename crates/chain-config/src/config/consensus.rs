use std::time::Duration;

use fuel_core_types::{
    fuel_tx::Input,
    fuel_types::Address,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::default_consensus_dev_key;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ConsensusConfig {
    PoA {
        signing_key: Address,
        /// If this time expires before receving new block - we consider to be synced with the chain
        time_until_synced: Duration,
        /// Minimum number of connected reserved peers to start block production
        min_connected_resereved_peers: usize,
        timeout_between_checking_peers: Duration,
    },
}

impl ConsensusConfig {
    pub fn default_poa() -> Self {
        ConsensusConfig::PoA {
            signing_key: Input::owner(&default_consensus_dev_key().public_key()),
            min_connected_resereved_peers: 4,
            time_until_synced: Duration::from_secs(60),
            timeout_between_checking_peers: Duration::from_millis(500),
        }
    }
}
