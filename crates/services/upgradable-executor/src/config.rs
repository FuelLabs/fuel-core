use fuel_core_executor::executor::ExecutionOptions;
use fuel_core_types::fuel_tx::ConsensusParameters;

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Network-wide common parameters used for validating the chain.
    /// The executor already has these parameters, and this field allows us
    /// to override the existing value.
    pub consensus_parameters: ConsensusParameters,
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,
    /// Default mode for utxo_validation
    pub utxo_validation_default: bool,
}

impl From<&Config> for ExecutionOptions {
    fn from(value: &Config) -> Self {
        Self {
            utxo_validation: value.utxo_validation_default,
            backtrace: value.backtrace,
            consensus_params: Some(value.consensus_parameters.clone()),
        }
    }
}
