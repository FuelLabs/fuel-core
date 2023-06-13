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
    PoA { signing_key: Address },
}

impl ConsensusConfig {
    pub fn default_poa() -> Self {
        ConsensusConfig::PoA {
            signing_key: Input::owner(&default_consensus_dev_key().public_key()),
        }
    }
}
