use std::sync::Arc;

use prometheus::Counter;
use slog::*;

use crate::components::metrics::MetricsRegistry;
use crate::components::store::DeploymentLocator;
use crate::log::elastic::*;
use crate::log::split::*;
use crate::prelude::ENV_VARS;

/// Configuration for component-specific logging to Elasticsearch.
pub struct ElasticComponentLoggerConfig {
    pub index: String,
}

/// Configuration for component-specific logging.
pub struct ComponentLoggerConfig {
    pub elastic: Option<ElasticComponentLoggerConfig>,
}

/// Factory for creating component and subgraph loggers.
#[derive(Clone)]
pub struct LoggerFactory {
    parent: Logger,
    elastic_config: Option<ElasticLoggingConfig>,
    metrics_registry: Arc<MetricsRegistry>,
}

impl LoggerFactory {
    /// Creates a new factory using a parent logger and optional Elasticsearch configuration.
    pub fn new(
        logger: Logger,
        elastic_config: Option<ElasticLoggingConfig>,
        metrics_registry: Arc<MetricsRegistry>,
    ) -> Self {
        Self {
            parent: logger,
            elastic_config,
            metrics_registry,
        }
    }

    /// Creates a new factory with a new parent logger.
    pub fn with_parent(&self, parent: Logger) -> Self {
        Self {
            parent,
            elastic_config: self.elastic_config.clone(),
            metrics_registry: self.metrics_registry.clone(),
        }
    }

    /// Creates a component-specific logger with optional Elasticsearch support.
    pub fn component_logger(
        &self,
        component: &str,
        config: Option<ComponentLoggerConfig>,
    ) -> Logger {
        let term_logger = self.parent.new(o!("component" => component.to_string()));

        match config {
            None => term_logger,
            Some(config) => match config.elastic {
                None => term_logger,
                Some(config) => self
                    .elastic_config
                    .clone()
                    .map(|elastic_config| {
                        split_logger(
                            term_logger.clone(),
                            elastic_logger(
                                ElasticDrainConfig {
                                    general: elastic_config,
                                    index: config.index,
                                    custom_id_key: String::from("componentId"),
                                    custom_id_value: component.to_string(),
                                    flush_interval: ENV_VARS.elastic_search_flush_interval,
                                    max_retries: ENV_VARS.elastic_search_max_retries,
                                },
                                term_logger.clone(),
                                self.logs_sent_counter(None),
                            ),
                        )
                    })
                    .unwrap_or(term_logger),
            },
        }
    }

    /// Creates a subgraph logger with Elasticsearch support.
    pub fn subgraph_logger(&self, loc: &DeploymentLocator) -> Logger {
        let term_logger = self
            .parent
            .new(o!("subgraph_id" => loc.hash.to_string(), "sgd" => loc.id.to_string()));

        self.elastic_config
            .clone()
            .map(|elastic_config| {
                split_logger(
                    term_logger.clone(),
                    elastic_logger(
                        ElasticDrainConfig {
                            general: elastic_config,
                            index: String::from("subgraph-logs"),
                            custom_id_key: String::from("subgraphId"),
                            custom_id_value: loc.hash.to_string(),
                            flush_interval: ENV_VARS.elastic_search_flush_interval,
                            max_retries: ENV_VARS.elastic_search_max_retries,
                        },
                        term_logger.clone(),
                        self.logs_sent_counter(Some(loc.hash.as_str())),
                    ),
                )
            })
            .unwrap_or(term_logger)
    }

    fn logs_sent_counter(&self, deployment: Option<&str>) -> Counter {
        self.metrics_registry
            .global_deployment_counter(
                "graph_elasticsearch_logs_sent",
                "Count of logs sent to Elasticsearch endpoint",
                deployment.unwrap_or(""),
            )
            .unwrap()
    }
}
