use crate::client::{
    schema,
    types::Block,
};
use fuel_core_types::{
    self,
    fuel_tx::ConsensusParameters,
};

pub struct ChainInfo {
    pub da_height: u64,
    pub name: String,
    pub latest_block: Block,
    pub consensus_parameters: ConsensusParameters,
}

// GraphQL Translation

impl From<schema::chain::ChainInfo> for ChainInfo {
    fn from(value: schema::chain::ChainInfo) -> Self {
        Self {
            da_height: value.da_height.into(),
            name: value.name,
            latest_block: value.latest_block.into(),
            consensus_parameters: value.consensus_parameters.into(),
        }
    }
}
