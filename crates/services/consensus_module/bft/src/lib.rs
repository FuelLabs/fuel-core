#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
pub mod service;

pub use config::Config;
pub use service::Service;
