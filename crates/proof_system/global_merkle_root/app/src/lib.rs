//! The CLI and adapters for the state root service

// #![deny(clippy::arithmetic_side_effects)]
// #![deny(clippy::cast_possible_truncation)]
// #![deny(unused_crate_dependencies)]
// #![deny(missing_docs)]
// #![deny(warnings)]

/// Adapters
pub mod adapters;

/// CLI definition
pub mod cli;

/// Application
pub mod app;

/// Integration tests
#[cfg(test)]
mod tests;
