#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
//! # Sync Service
//! Responsible for syncing the blockchain from the network.

pub mod import;
pub mod ports;
pub mod service;
mod state;
pub mod sync;
