use fuel_core_types::fuel_tx::ConsensusParameters;
use fuel_core_wasm_executor::fuel_core_executor::executor::ExecutionOptions;

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Network-wide common parameters used for validating the chain.
    /// The executor already has these parameters, and this field allows override the used value.
    pub consensus_parameters: Option<ConsensusParameters>,
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
            consensus_params: value.consensus_parameters.clone(),
        }
    }
}
