#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use prometheus_client::{
    encoding::text::encode,
    registry::Registry,
};
use std::{
    ops::Deref,
    sync::OnceLock,
};

/// The global register of all metrics.
#[derive(Default)]
pub struct GlobalRegistry {
    // It is okay to use std mutex because we register each metric only one time.
    pub registry: parking_lot::Mutex<Registry>,
}

mod buckets;
pub mod core_metrics;
pub mod future_tracker;
pub mod graphql_metrics;
pub mod importer;
pub mod p2p_metrics;
pub mod services;
pub mod txpool_metrics;

static GLOBAL_REGISTER: OnceLock<GlobalRegistry> = OnceLock::new();

pub fn global_registry() -> &'static GlobalRegistry {
    GLOBAL_REGISTER.get_or_init(GlobalRegistry::default)
}

pub fn encode_metrics() -> Result<String, std::fmt::Error> {
    let mut encoded = String::new();

    // encode the rest of the fuel-core metrics using latest prometheus
    let registry = global_registry().registry.lock();
    encode(&mut encoded, registry.deref())?;

    Ok(encoded)
}
