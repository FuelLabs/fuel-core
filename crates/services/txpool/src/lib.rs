#![deny(unused_crate_dependencies)]

pub mod config;
mod containers;
pub mod ports;
pub mod service;
mod transaction_selector;
pub mod txpool;
pub mod types;

#[cfg(any(test, feature = "test-helpers"))]
pub mod mock_db;
#[cfg(any(test, feature = "test-helpers"))]
pub use mock_db::MockDb;

pub use config::Config;
pub use fuel_core_types::services::txpool::Error;
pub use service::{
    Service,
    ServiceBuilder,
};
pub use txpool::TxPool;

#[cfg(any(test, feature = "test-helpers"))]
pub(crate) mod test_helpers;
