use diesel::result::Error as DieselError;
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
mod database;
mod manifest;
mod schema;
pub mod executor;

pub use database::SchemaManager;
pub use executor::{IndexEnv, IndexExecutor};
pub use manifest::Manifest;

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
    #[error("FFI Error {0:?}")]
    FFIError(#[from] ffi::FFIError),
    #[error("Database initialization error: {0:?}")]
    DatabaseInitError(#[from] r2d2::Error),
    #[error("Database query error: {0:?}")]
    DatabaseQueryError(#[from] DieselError),
    #[error("Missing handler: {0:?}")]
    MissingHandler(String),
    #[error("Unknown error")]
    Unknown,
}


