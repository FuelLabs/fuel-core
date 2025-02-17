//! The state root service

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

/// Port definitions
pub mod ports;

/// Service definition
pub mod service;

#[cfg(test)]
mod tests;
