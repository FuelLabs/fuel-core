use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    tai64::Tai64,
};

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub da_height: DaBlockHeight,
    pub prev_root: Bytes32,
    pub height: BlockHeight,
    pub time: Tai64,
}
