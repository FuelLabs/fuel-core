use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use prometheus::{labels, Histogram, IntCounterVec};

use crate::components::metrics::{counter_with_labels, gauge_with_labels};
use crate::prelude::Collector;
use crate::prometheus::{
    Counter, CounterVec, Error as PrometheusError, Gauge, GaugeVec, HistogramOpts, HistogramVec,
    Opts, Registry,
};
use crate::slog::{self, error, o, Logger};

pub struct MetricsRegistry {
    logger: Logger,
    registry: Arc<Registry>,
    register_errors: Box<Counter>,
    unregister_errors: Box<Counter>,
    registered_metrics: Box<Gauge>,

    /// Global metrics are lazily initialized and identified by
    /// the `Desc.id` that hashes the name and const label values
    global_counters: RwLock<HashMap<u64, Counter>>,
    global_counter_vecs: RwLock<HashMap<u64, CounterVec>>,
    global_gauges: RwLock<HashMap<u64, Gauge>>,
    global_gauge_vecs: RwLock<HashMap<u64, GaugeVec>>,
    global_histogram_vecs: RwLock<HashMap<u64, HistogramVec>>,
}

impl MetricsRegistry {
    pub fn new(logger: Logger, registry: Arc<Registry>) -> Self {
        // Generate internal metrics
        let register_errors = Self::gen_register_errors_counter(registry.clone());
        let unregister_errors = Self::gen_unregister_errors_counter(registry.clone());
        let registered_metrics = Self::gen_registered_metrics_gauge(registry.clone());

        MetricsRegistry {
            logger: logger.new(o!("component" => String::from("MetricsRegistry"))),
            registry,
            register_errors,
            unregister_errors,
            registered_metrics,
            global_counters: RwLock::new(HashMap::new()),
            global_counter_vecs: RwLock::new(HashMap::new()),
            global_gauges: RwLock::new(HashMap::new()),
            global_gauge_vecs: RwLock::new(HashMap::new()),
            global_histogram_vecs: RwLock::new(HashMap::new()),
        }
    }

    pub fn mock() -> Self {
        MetricsRegistry::new(Logger::root(slog::Discard, o!()), Arc::new(Registry::new()))
    }

    fn gen_register_errors_counter(registry: Arc<Registry>) -> Box<Counter> {
        let opts = Opts::new(
            String::from("metrics_register_errors"),
            String::from("Counts Prometheus metrics register errors"),
        );
        let counter = Box::new(
            Counter::with_opts(opts).expect("failed to create `metrics_register_errors` counter"),
        );
        registry
            .register(counter.clone())
            .expect("failed to register `metrics_register_errors` counter");
        counter
    }

    fn gen_unregister_errors_counter(registry: Arc<Registry>) -> Box<Counter> {
        let opts = Opts::new(
            String::from("metrics_unregister_errors"),
            String::from("Counts Prometheus metrics unregister errors"),
        );
        let counter = Box::new(
            Counter::with_opts(opts).expect("failed to create `metrics_unregister_errors` counter"),
        );
        registry
            .register(counter.clone())
            .expect("failed to register `metrics_unregister_errors` counter");
        counter
    }

    fn gen_registered_metrics_gauge(registry: Arc<Registry>) -> Box<Gauge> {
        let opts = Opts::new(
            String::from("registered_metrics"),
            String::from("Tracks the number of registered metrics on the node"),
        );
        let gauge =
            Box::new(Gauge::with_opts(opts).expect("failed to create `registered_metrics` gauge"));
        registry
            .register(gauge.clone())
            .expect("failed to register `registered_metrics` gauge");
        gauge
    }

    fn global_counter_vec_internal(
        &self,
        name: &str,
        help: &str,
        deployment: Option<&str>,
        variable_labels: &[&str],
    ) -> Result<CounterVec, PrometheusError> {
        let opts = Opts::new(name, help);
        let opts = match deployment {
            None => opts,
            Some(deployment) => opts.const_label("deployment", deployment),
        };
        let counters = CounterVec::new(opts, variable_labels)?;
        let id = counters.desc().first().unwrap().id;
        let maybe_counter = self.global_counter_vecs.read().unwrap().get(&id).cloned();
        if let Some(counters) = maybe_counter {
            Ok(counters)
        } else {
            self.register(name, Box::new(counters.clone()));
            self.global_counter_vecs
                .write()
                .unwrap()
                .insert(id, counters.clone());
            Ok(counters)
        }
    }

    pub fn register(&self, name: &str, c: Box<dyn Collector>) {
        let err = match self.registry.register(c).err() {
            None => {
                self.registered_metrics.inc();
                return;
            }
            Some(err) => {
                self.register_errors.inc();
                err
            }
        };
        match err {
            PrometheusError::AlreadyReg => {
                error!(
                    self.logger,
                    "registering metric [{}] failed because it was already registered", name,
                );
            }
            PrometheusError::InconsistentCardinality { expect, got } => {
                error!(
                    self.logger,
                    "registering metric [{}] failed due to inconsistent caridinality, expected = {} got = {}",
                    name,
                    expect,
                    got,
                );
            }
            PrometheusError::Msg(msg) => {
                error!(
                    self.logger,
                    "registering metric [{}] failed because: {}", name, msg,
                );
            }
            PrometheusError::Io(err) => {
                error!(
                    self.logger,
                    "registering metric [{}] failed due to io error: {}", name, err,
                );
            }
            PrometheusError::Protobuf(err) => {
                error!(
                    self.logger,
                    "registering metric [{}] failed due to protobuf error: {}", name, err
                );
            }
        };
    }

    pub fn global_counter(
        &self,
        name: &str,
        help: &str,
        const_labels: HashMap<String, String>,
    ) -> Result<Counter, PrometheusError> {
        let counter = counter_with_labels(name, help, const_labels)?;
        let id = counter.desc().first().unwrap().id;
        let maybe_counter = self.global_counters.read().unwrap().get(&id).cloned();
        if let Some(counter) = maybe_counter {
            Ok(counter)
        } else {
            self.register(name, Box::new(counter.clone()));
            self.global_counters
                .write()
                .unwrap()
                .insert(id, counter.clone());
            Ok(counter)
        }
    }

    pub fn global_deployment_counter(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
    ) -> Result<Counter, PrometheusError> {
        self.global_counter(name, help, deployment_labels(subgraph))
    }

    pub fn global_counter_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: &[&str],
    ) -> Result<CounterVec, PrometheusError> {
        self.global_counter_vec_internal(name, help, None, variable_labels)
    }

    pub fn global_deployment_counter_vec(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
        variable_labels: &[&str],
    ) -> Result<CounterVec, PrometheusError> {
        self.global_counter_vec_internal(name, help, Some(subgraph), variable_labels)
    }

    pub fn global_gauge(
        &self,
        name: &str,
        help: &str,
        const_labels: HashMap<String, String>,
    ) -> Result<Gauge, PrometheusError> {
        let gauge = gauge_with_labels(name, help, const_labels)?;
        let id = gauge.desc().first().unwrap().id;
        let maybe_gauge = self.global_gauges.read().unwrap().get(&id).cloned();
        if let Some(gauge) = maybe_gauge {
            Ok(gauge)
        } else {
            self.register(name, Box::new(gauge.clone()));
            self.global_gauges
                .write()
                .unwrap()
                .insert(id, gauge.clone());
            Ok(gauge)
        }
    }

    pub fn global_gauge_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: &[&str],
    ) -> Result<GaugeVec, PrometheusError> {
        let opts = Opts::new(name, help);
        let gauges = GaugeVec::new(opts, variable_labels)?;
        let id = gauges.desc().first().unwrap().id;
        let maybe_gauge = self.global_gauge_vecs.read().unwrap().get(&id).cloned();
        if let Some(gauges) = maybe_gauge {
            Ok(gauges)
        } else {
            self.register(name, Box::new(gauges.clone()));
            self.global_gauge_vecs
                .write()
                .unwrap()
                .insert(id, gauges.clone());
            Ok(gauges)
        }
    }

    pub fn global_histogram_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: &[&str],
    ) -> Result<HistogramVec, PrometheusError> {
        let opts = HistogramOpts::new(name, help);
        let histograms = HistogramVec::new(opts, variable_labels)?;
        let id = histograms.desc().first().unwrap().id;
        let maybe_histogram = self.global_histogram_vecs.read().unwrap().get(&id).cloned();
        if let Some(histograms) = maybe_histogram {
            Ok(histograms)
        } else {
            self.register(name, Box::new(histograms.clone()));
            self.global_histogram_vecs
                .write()
                .unwrap()
                .insert(id, histograms.clone());
            Ok(histograms)
        }
    }

    pub fn unregister(&self, metric: Box<dyn Collector>) {
        match self.registry.unregister(metric) {
            Ok(_) => {
                self.registered_metrics.dec();
            }
            Err(e) => {
                self.unregister_errors.inc();
                error!(self.logger, "Unregistering metric failed = {:?}", e,);
            }
        };
    }

    pub fn new_gauge(
        &self,
        name: &str,
        help: &str,
        const_labels: HashMap<String, String>,
    ) -> Result<Box<Gauge>, PrometheusError> {
        let opts = Opts::new(name, help).const_labels(const_labels);
        let gauge = Box::new(Gauge::with_opts(opts)?);
        self.register(name, gauge.clone());
        Ok(gauge)
    }

    pub fn new_deployment_gauge(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
    ) -> Result<Gauge, PrometheusError> {
        let opts = Opts::new(name, help).const_labels(deployment_labels(subgraph));
        let gauge = Gauge::with_opts(opts)?;
        self.register(name, Box::new(gauge.clone()));
        Ok(gauge)
    }

    pub fn new_gauge_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: Vec<String>,
    ) -> Result<Box<GaugeVec>, PrometheusError> {
        let opts = Opts::new(name, help);
        let gauges = Box::new(GaugeVec::new(
            opts,
            variable_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
        )?);
        self.register(name, gauges.clone());
        Ok(gauges)
    }

    pub fn new_deployment_gauge_vec(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
        variable_labels: Vec<String>,
    ) -> Result<Box<GaugeVec>, PrometheusError> {
        let opts = Opts::new(name, help).const_labels(deployment_labels(subgraph));
        let gauges = Box::new(GaugeVec::new(
            opts,
            variable_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
        )?);
        self.register(name, gauges.clone());
        Ok(gauges)
    }

    pub fn new_counter(&self, name: &str, help: &str) -> Result<Box<Counter>, PrometheusError> {
        let opts = Opts::new(name, help);
        let counter = Box::new(Counter::with_opts(opts)?);
        self.register(name, counter.clone());
        Ok(counter)
    }

    pub fn new_counter_with_labels(
        &self,
        name: &str,
        help: &str,
        const_labels: HashMap<String, String>,
    ) -> Result<Box<Counter>, PrometheusError> {
        let counter = Box::new(counter_with_labels(name, help, const_labels)?);
        self.register(name, counter.clone());
        Ok(counter)
    }

    pub fn new_deployment_counter(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
    ) -> Result<Counter, PrometheusError> {
        let counter = counter_with_labels(name, help, deployment_labels(subgraph))?;
        self.register(name, Box::new(counter.clone()));
        Ok(counter)
    }

    pub fn new_int_counter_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: &[&str],
    ) -> Result<Box<IntCounterVec>, PrometheusError> {
        let opts = Opts::new(name, help);
        let counters = Box::new(IntCounterVec::new(opts, &variable_labels)?);
        self.register(name, counters.clone());
        Ok(counters)
    }

    pub fn new_counter_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: Vec<String>,
    ) -> Result<Box<CounterVec>, PrometheusError> {
        let opts = Opts::new(name, help);
        let counters = Box::new(CounterVec::new(
            opts,
            variable_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
        )?);
        self.register(name, counters.clone());
        Ok(counters)
    }

    pub fn new_deployment_counter_vec(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
        variable_labels: Vec<String>,
    ) -> Result<Box<CounterVec>, PrometheusError> {
        let opts = Opts::new(name, help).const_labels(deployment_labels(subgraph));
        let counters = Box::new(CounterVec::new(
            opts,
            variable_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
        )?);
        self.register(name, counters.clone());
        Ok(counters)
    }

    pub fn new_deployment_histogram(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
        buckets: Vec<f64>,
    ) -> Result<Box<Histogram>, PrometheusError> {
        let opts = HistogramOpts::new(name, help)
            .const_labels(deployment_labels(subgraph))
            .buckets(buckets);
        let histogram = Box::new(Histogram::with_opts(opts)?);
        self.register(name, histogram.clone());
        Ok(histogram)
    }

    pub fn new_histogram(
        &self,
        name: &str,
        help: &str,
        buckets: Vec<f64>,
    ) -> Result<Box<Histogram>, PrometheusError> {
        let opts = HistogramOpts::new(name, help).buckets(buckets);
        let histogram = Box::new(Histogram::with_opts(opts)?);
        self.register(name, histogram.clone());
        Ok(histogram)
    }

    pub fn new_histogram_vec(
        &self,
        name: &str,
        help: &str,
        variable_labels: Vec<String>,
        buckets: Vec<f64>,
    ) -> Result<Box<HistogramVec>, PrometheusError> {
        let opts = Opts::new(name, help);
        let histograms = Box::new(HistogramVec::new(
            HistogramOpts {
                common_opts: opts,
                buckets,
            },
            variable_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
        )?);
        self.register(name, histograms.clone());
        Ok(histograms)
    }

    pub fn new_deployment_histogram_vec(
        &self,
        name: &str,
        help: &str,
        subgraph: &str,
        variable_labels: Vec<String>,
        buckets: Vec<f64>,
    ) -> Result<Box<HistogramVec>, PrometheusError> {
        let opts = Opts::new(name, help).const_labels(deployment_labels(subgraph));
        let histograms = Box::new(HistogramVec::new(
            HistogramOpts {
                common_opts: opts,
                buckets,
            },
            variable_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
        )?);
        self.register(name, histograms.clone());
        Ok(histograms)
    }
}

fn deployment_labels(subgraph: &str) -> HashMap<String, String> {
    labels! { String::from("deployment") => String::from(subgraph), }
}

#[test]
fn global_counters_are_shared() {
    use crate::log;

    let logger = log::logger(false);
    let prom_reg = Arc::new(Registry::new());
    let registry = MetricsRegistry::new(logger, prom_reg);

    fn check_counters(
        registry: &MetricsRegistry,
        name: &str,
        const_labels: HashMap<String, String>,
    ) {
        let c1 = registry
            .global_counter(name, "help me", const_labels.clone())
            .expect("first test counter");
        let c2 = registry
            .global_counter(name, "help me", const_labels)
            .expect("second test counter");
        let desc1 = c1.desc();
        let desc2 = c2.desc();
        let d1 = desc1.first().unwrap();
        let d2 = desc2.first().unwrap();

        // Registering the same metric with the same name and
        // const labels twice works and returns the same metric (logically)
        assert_eq!(d1.id, d2.id, "counters: {}", name);

        // They share the reported values
        c1.inc_by(7.0);
        c2.inc_by(2.0);
        assert_eq!(9.0, c1.get(), "counters: {}", name);
        assert_eq!(9.0, c2.get(), "counters: {}", name);
    }

    check_counters(&registry, "nolabels", HashMap::new());

    let const_labels = {
        let mut map = HashMap::new();
        map.insert("pool".to_owned(), "main".to_owned());
        map
    };
    check_counters(&registry, "pool", const_labels);

    let const_labels = {
        let mut map = HashMap::new();
        map.insert("pool".to_owned(), "replica0".to_owned());
        map
    };
    check_counters(&registry, "pool", const_labels);
}
