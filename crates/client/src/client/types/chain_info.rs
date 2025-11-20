use crate::client::{schema, schema::ConversionError, types::Block};
use fuel_core_types::{self, fuel_tx::ConsensusParameters};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainInfo {
    pub da_height: u64,
    pub name: String,
    pub latest_block: Block,
    pub consensus_parameters: ConsensusParameters,
}

// GraphQL Translation

impl TryFrom<schema::chain::ChainInfo> for ChainInfo {
    type Error = ConversionError;

    fn try_from(value: schema::chain::ChainInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            da_height: value.da_height.into(),
            name: value.name,
            latest_block: value.latest_block.try_into()?,
            consensus_parameters: value.consensus_parameters.try_into()?,
        })
    }
}
