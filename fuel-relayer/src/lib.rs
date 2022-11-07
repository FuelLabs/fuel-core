//! # Fuel Relayer

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub(crate) mod abi;
pub(crate) mod config;
pub(crate) mod log;

mod relayer;

#[cfg(any(test, feature = "test-helpers"))]
pub mod mock_db;
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

#[cfg(any(test, feature = "test-helpers"))]
pub use abi::*;

pub use config::Config;
pub use ethers_core::types::{
    H160,
    H256,
};
pub use relayer::{
    RelayerHandle,
    RelayerSynced,
};
