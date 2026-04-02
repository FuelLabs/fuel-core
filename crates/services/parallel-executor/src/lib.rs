#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]

pub mod config;
pub mod executor;
pub(crate) mod in_memory_transaction_with_contracts;
pub(crate) mod l1_execution_data;
pub mod ports;

mod memory;
mod once_transaction_source;
mod tx_waiter;

#[cfg(test)]
mod tests;

pub mod scheduler;
