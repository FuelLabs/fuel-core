#![deny(unused_crate_dependencies)]
#![deny(warnings)]
pub mod client;
#[cfg(feature = "dap")]
pub mod schema;
