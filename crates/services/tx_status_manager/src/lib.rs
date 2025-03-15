//! This crate provides a service for managing transaction statuses.

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
mod error;
mod manager;
pub mod ports;
pub mod service;
mod subscriptions;
mod tx_status_stream;
mod update_sender;
pub mod utils;

pub use manager::TxStatusManager;
pub use service::{
    new_service,
    SharedData,
    Task,
};
pub use tx_status_stream::{
    TxStatusMessage,
    TxStatusStream,
};
pub use utils::from_executor_to_status;

#[cfg(test)]
mod tests;
