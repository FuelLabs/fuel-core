pub mod config;
mod containers;
pub use fuel_core_interfaces::txpool::Error;

mod interface;
pub mod service;
pub mod txpool;
pub mod types;

pub use config::Config;
pub use service::Service;
pub use txpool::TxPool;
