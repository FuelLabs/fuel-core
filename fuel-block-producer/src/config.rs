use fuel_core_interfaces::common::fuel_tx::ConsensusParameters;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub utxo_validation: bool,
    pub consensus_params: ConsensusParameters,
}
