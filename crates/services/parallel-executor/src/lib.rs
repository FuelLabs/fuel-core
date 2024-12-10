#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
mod dependency_splitter;
pub mod executor;
mod one_core_tx_source;
