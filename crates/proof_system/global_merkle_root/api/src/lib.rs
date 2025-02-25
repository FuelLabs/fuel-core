//! The GraphQL API of the state root service

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

/// Port definitions
pub mod ports;

/// API service definition
pub mod service;

/// GraphQL schema definition
pub mod schema;

#[cfg(test)]
pub mod tests;
