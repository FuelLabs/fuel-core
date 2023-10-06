use prometheus_client::{
    encoding::text::encode,
    metrics::counter::Counter,
    registry::Registry,
};
use std::{
    ops::Deref,
    sync::{
        Mutex,
        OnceLock,
    },
};

/// The statistic of the service life cycle.
#[derive(Default, Debug)]
pub struct ServiceLifecycle {
    /// The time spent for real actions by the service.
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

/// The register of the metrics for each service.
#[derive(Default)]
pub struct ServicesMetrics {
    // It is okay to use std mutex because we register each service only one time.
    pub registry: Mutex<Registry>,
}

impl ServicesMetrics {
    pub fn register_service(&self, service_name: &str) -> ServiceLifecycle {
        let reg =
            regex::Regex::new("^[a-zA-Z_:][a-zA-Z0-9_:]*$").expect("It is a valid Regex");
        if !reg.is_match(service_name) {
            panic!("The service {} has incorrect name.", service_name);
        }
        let lifecycle = ServiceLifecycle::default();
        let mut lock = self
            .registry
            .lock()
            .expect("The lock of the service metric is poisoned");

        // Check that it is a unique service.
        let mut encoded_bytes = String::new();
        encode(&mut encoded_bytes, lock.deref())
            .expect("Unable to decode service metrics");

        let reg = regex::Regex::new(format!("\\b{}\\b", service_name).as_str())
            .expect("It is a valid Regex");
        if reg.is_match(encoded_bytes.as_str()) {
            tracing::warn!("Service with '{}' name is already registered", service_name);
        }

        lock.register(
            format!("{}_idle_ns", service_name),
            format!("The idle time of the {} service", service_name),
            lifecycle.idle.clone(),
        );
        lock.register(
            format!("{}_busy_ns", service_name),
            format!("The busy time of the {} service", service_name),
            lifecycle.busy.clone(),
        );

        lifecycle
    }
}

static SERVICES_METRICS: OnceLock<ServicesMetrics> = OnceLock::new();

pub fn services_metrics() -> &'static ServicesMetrics {
    SERVICES_METRICS.get_or_init(ServicesMetrics::default)
}

#[test]
fn register_success() {
    services_metrics().register_service("Foo");
    services_metrics().register_service("Bar");
}
