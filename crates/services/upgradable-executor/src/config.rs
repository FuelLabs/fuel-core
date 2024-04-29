use fuel_core_executor::executor::ExecutionOptions;
use fuel_core_types::blockchain::header::StateTransitionBytecodeVersion;

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,
    /// Default mode for utxo_validation
    pub utxo_validation_default: bool,
    /// The version of the native executor to determine usage of native vs WASM executor.
    /// If it is `None`, the `Executor::VERSION` is used.
    ///
    /// When a block version matches the native executor version, we use
    /// the native executor; otherwise, we use the WASM executor.
    pub native_executor_version: Option<StateTransitionBytecodeVersion>,
}

impl From<&Config> for ExecutionOptions {
    fn from(value: &Config) -> Self {
        Self {
            extra_tx_checks: value.utxo_validation_default,
            backtrace: value.backtrace,
        }
    }
}
