//! # Fuel Relayer

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
// #![deny(unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(warnings)]

pub(crate) mod abi;
pub(crate) mod config;
pub(crate) mod log;

mod service;

#[cfg(any(test, feature = "test-helpers"))]
pub mod mock_db;
pub mod ports;
pub mod storage;
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

#[cfg(any(test, feature = "test-helpers"))]
pub use abi::*;
#[cfg(any(test, feature = "test-helpers"))]
pub use service::new_service_test;

pub use alloy_primitives::{
    Address,
    B256,
};
pub use config::Config;
pub use service::{
    Service,
    SharedState,
    new_service,
};

#[cfg(test)]
fuel_core_trace::enable_tracing!();
