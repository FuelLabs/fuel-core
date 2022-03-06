use diesel::result::Error as DieselError;
use thiserror::Error;
use wasmer::{ExportError, HostEnvInitError, InstantiationError, RuntimeError};

pub mod api;
mod database;
pub mod executor;
mod ffi;
mod manifest;
mod service;

pub use api::GraphQlApi;
pub use database::SchemaManager;
pub use executor::{IndexEnv, IndexExecutor};
pub use manifest::Manifest;
pub use service::{IndexerConfig, IndexerService};

pub type IndexerResult<T> = core::result::Result<T, IndexerError>;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("Compiler error: {0:#?}")]
    CompileError(#[from] wasmer::CompileError),
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
    #[error("Database connection error: {0:?}")]
    ConnectionError(#[from] diesel::ConnectionError),
    #[error("Missing handler: {0:?}")]
    MissingHandler(String),
    #[error("Indexer transaction error {0:?}")]
    TxError(#[from] crate::executor::TxError),
    #[error("Unknown error")]
    Unknown,
}
