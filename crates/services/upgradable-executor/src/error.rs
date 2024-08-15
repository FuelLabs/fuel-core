/// Error type for the upgradable (wasm) executor.
#[cfg(feature = "wasm-executor")]
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, derive_more::Display, derive_more::From)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UpgradableError {
    #[display(fmt = "Invalid WASM bytecode: {_0} (version: {_1:?})")]
    #[cfg(feature = "wasm-executor")]
    InvalidWasm(
        String,
        Option<fuel_core_types::blockchain::header::StateTransitionBytecodeVersion>,
    ),
    /// Normal errors from the executor
    #[display(fmt = "Executor error: {_0}")]
    ExecutorError(fuel_core_types::services::executor::Error),
}
