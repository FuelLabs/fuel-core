//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
//#![deny(missing_docs)] // TODO: Reenable
#![deny(unused_crate_dependencies)]
//#![deny(warnings)] // TODO: Reenable
#![deny(unused_variables)]

/// Database implementations
pub mod state;

/// Database description types
pub mod database_description;

/// Database error type
pub mod error;

pub use error::Error;
pub type Result<T> = core::result::Result<T, Error>;
