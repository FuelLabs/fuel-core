use fuel_core_interfaces::common::fuel_tx::ConsensusParameters;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub consensus_params: ConsensusParameters,
}
