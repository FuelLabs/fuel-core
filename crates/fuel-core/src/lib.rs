#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[doc(no_inline)]
pub use fuel_core_chain_config as chain_config;
#[cfg(feature = "p2p")]
#[doc(no_inline)]
pub use fuel_core_p2p as p2p;
#[doc(no_inline)]
pub use fuel_core_producer as producer;
#[cfg(feature = "relayer")]
#[doc(no_inline)]
pub use fuel_core_relayer as relayer;
#[cfg(feature = "p2p")]
#[doc(no_inline)]
pub use fuel_core_sync as sync;
#[doc(no_inline)]
pub use fuel_core_txpool as txpool;
#[doc(no_inline)]
pub use fuel_core_types as types;

pub mod coins_query;
pub mod combined_database;
pub mod database;
pub mod executor;
pub mod model;
#[cfg(all(feature = "p2p", feature = "test-helpers"))]
pub mod p2p_test_helpers;
pub mod query;
pub mod schema;
pub mod service;
pub mod state;

// In the future this module will be a separate crate for `fuel-core-graphql-api`.
mod graphql_api;

pub mod fuel_core_graphql_api {
    pub use crate::graphql_api::*;
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
