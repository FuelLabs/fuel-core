#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]

pub(crate) mod checked_transaction_ext;
pub(crate) mod column_adapter;
pub mod config;
pub mod executor;
pub(crate) mod l1_execution_data;
pub mod ports;
pub(crate) mod txs_ext;

mod coin;
mod once_transaction_source;
mod tx_waiter;
mod validator;

#[cfg(test)]
mod tests;

pub mod scheduler;
