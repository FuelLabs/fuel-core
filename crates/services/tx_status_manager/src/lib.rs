//! This crate ...
mod manager;
pub mod ports;
mod service;
mod shared_state;
mod subscriptions;

pub use manager::TxStatusManager;
pub use service::{
    new_service,
    Task,
};

//#[cfg(test)]
// mod tests;
