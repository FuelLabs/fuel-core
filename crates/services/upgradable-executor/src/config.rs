use fuel_core_executor::executor::ExecutionOptions;

#[derive(Clone, Debug, Default)]
pub struct Config {
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
        }
    }
}
