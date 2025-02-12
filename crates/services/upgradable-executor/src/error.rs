/// Error type for the upgradable (wasm) executor.
#[cfg(feature = "wasm-executor")]
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, derive_more::Display, derive_more::From)]
pub enum UpgradableError {
    #[display(fmt = "Invalid WASM bytecode: {_0}")]
    #[cfg(feature = "wasm-executor")]
    InvalidWasm(String),
    #[display(fmt = "The uploaded bytecode with root {_0} is incomplete")]
    IncompleteUploadedBytecode(fuel_core_types::fuel_tx::Bytes32),
    /// Normal errors from the executor
    #[display(fmt = "Executor error: {_0}")]
    ExecutorError(fuel_core_types::services::executor::Error),
}
