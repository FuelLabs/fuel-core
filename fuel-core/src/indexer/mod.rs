use thiserror::Error;
use wasmer::{
    ExportError,
    HostEnvInitError,
    InstantiationError,
    RuntimeError,
};
use wasmer_wasi::{
    WasiError,
    WasiStateCreationError
};

mod ffi;
pub mod runtime;

pub use runtime::{IndexEnv, IndexExecutor};

pub type IndexerResult<T> = core::result::Result<T, IndexerError>;


#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("Compiler error: {0:#?}")]
    CompileError(#[from] wasmer::CompileError),
    #[error("Error setting up wasi state {0:#?}")]
    WasiStateError(#[from] WasiStateCreationError),
    #[error("Module creation error: {0:#?}")]
    WasiError(#[from] WasiError),
    #[error("Error instantiating wasm interpreter: {0:#?}")]
    InstantiationError(#[from] InstantiationError),
    #[error("Error finding exported symbol: {0:#?}")]
    ExportError(#[from] ExportError),
    #[error("Error executing function: {0:#?}")]
    RuntimeError(#[from] RuntimeError),
    #[error("Could not initialize host environment: {0:#?}")]
    HostEnvInitError(#[from] HostEnvInitError),
    #[error("Unknown error")]
    Unknown,
}


