#![deny(unused_crate_dependencies)]

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

pub mod database;
pub mod executor;
pub mod model;
mod query;
pub mod resource_query;
pub mod schema;
pub mod service;
pub mod state;

// In the future this module will be a separate crate for `fuel-core-graphql-api`.
mod graphql_api;

pub mod fuel_core_graphql_api {
    pub use crate::graphql_api::*;
}
