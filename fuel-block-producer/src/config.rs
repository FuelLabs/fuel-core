use fuel_core_interfaces::common::{
    fuel_tx::ConsensusParameters,
    fuel_types::Word,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub max_gas_per_block: Word,
    pub consensus_params: ConsensusParameters,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // TODO: pick a reasonable default value based on gas scheduling analysis
            max_gas_per_block: 10 * ConsensusParameters::DEFAULT.max_gas_per_tx,
            consensus_params: Default::default(),
        }
    }
}
