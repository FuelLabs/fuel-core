//#![deny(clippy::arithmetic_side_effects)]
//#![deny(clippy::cast_possible_truncation)]
//#![deny(unused_crate_dependencies)]
//#![deny(missing_docs)]
//#![deny(warnings)]
//! # Sync Service
//! Responsible for syncing the blockchain from the network.

pub mod import;
pub mod ports;
pub mod service;
pub mod state;
pub mod sync;
mod tracing_helpers;

pub use import::Config;

use rand as _;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
