#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[allow(unused)]
pub mod static_updater;

pub mod ports;
pub mod v0;

pub mod common;

pub mod sync_state;
#[allow(unused)]
pub mod v1;
