//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![deny(missing_docs)]

pub use fuel_vm_private::fuel_storage::*;

pub mod tables;
