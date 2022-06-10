pub mod config;
mod containers;
pub use fuel_core_interfaces::txpool::Error;

pub mod service;
mod interface;
pub mod txpool;
pub mod types;

pub use config::Config;
pub use service::Service;
pub use txpool::TxPool;
