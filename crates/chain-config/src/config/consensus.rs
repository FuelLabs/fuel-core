use fuel_core_types::fuel_types::Address;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ConsensusConfig {
    PoA { signing_key: Address },
}
