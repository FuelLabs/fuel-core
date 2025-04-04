//! # Compression Service

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

/// Configuration for the compression service
pub mod config;
/// Error types for the compression service
pub mod errors;
pub(crate) mod metrics;
/// Ports for the compression service
pub mod ports;
/// Service definition for the compression service
pub mod service;
/// Storage traits for the compression service
pub mod storage;
/// Sync state for the compression service
pub mod sync_state;
/// Temporal Registry implementations
pub mod temporal_registry;

/// Result type for compression operations
pub type Result<T> = core::result::Result<T, errors::CompressionError>;
