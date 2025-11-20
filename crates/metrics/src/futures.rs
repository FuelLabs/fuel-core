use crate::global_registry;
use prometheus_client::{encoding::text::encode, metrics::counter::Counter};
use std::ops::Deref;

pub mod future_tracker;
pub mod metered_future;

/// The statistic of the futures life cycle.
#[derive(Default, Debug, Clone)]
pub struct FuturesMetrics {
    /// The time spent for real actions by the future.
    ///
    /// Time is in nanoseconds.
    // TODO: Use `AtomicU128` when it is stable, otherwise, the field can overflow at some point.
    pub busy: Counter,
    /// The idle time of awaiting sub-tasks or any action from the system/user.
    ///
    /// Time is in nanoseconds.
    // TODO: Use `AtomicU128` when it is stable, otherwise, the field can overflow at some point.
    pub idle: Counter,
}

impl FuturesMetrics {
    pub fn obtain_futures_metrics(futures_name: &str) -> FuturesMetrics {
        let reg =
            regex::Regex::new("^[a-zA-Z_:][a-zA-Z0-9_:]*$").expect("It is a valid Regex");
        if !reg.is_match(futures_name) {
            panic!("The futures metric {} has incorrect name.", futures_name);
        }
        let lifecycle = FuturesMetrics::default();
        let mut lock = global_registry().registry.lock();

        // Check that it is a unique futures.
        let mut encoded_bytes = String::new();
        encode(&mut encoded_bytes, lock.deref())
            .expect("Unable to decode futures metrics");

        let reg = regex::Regex::new(format!("\\b{}\\b", futures_name).as_str())
            .expect("It is a valid Regex");
        if reg.is_match(encoded_bytes.as_str()) {
            tracing::warn!(
                "Futures metrics with '{}' name is already registered",
                futures_name
            );
        }

        lock.register(
            format!("{}_idle_ns", futures_name),
            format!("The idle time of the {} future", futures_name),
            lifecycle.idle.clone(),
        );
        lock.register(
            format!("{}_busy_ns", futures_name),
            format!("The busy time of the {} future", futures_name),
            lifecycle.busy.clone(),
        );

        lifecycle
    }
}

#[test]
fn register_success() {
    FuturesMetrics::obtain_futures_metrics("Foo");
    FuturesMetrics::obtain_futures_metrics("Bar");
}
