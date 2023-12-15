use prometheus::Counter;

use crate::blockchain::block_stream::BlockStreamMetrics;
use crate::prelude::{Gauge, Histogram, HostMetrics};
use std::collections::HashMap;
use std::sync::Arc;

use super::stopwatch::StopwatchMetrics;
use super::MetricsRegistry;

pub struct SubgraphInstanceMetrics {
    pub block_trigger_count: Box<Histogram>,
    pub block_processing_duration: Box<Histogram>,
    pub block_ops_transaction_duration: Box<Histogram>,
    pub firehose_connection_errors: Counter,

    pub stopwatch: StopwatchMetrics,
    trigger_processing_duration: Box<Histogram>,
}

impl SubgraphInstanceMetrics {
    pub fn new(
        registry: Arc<MetricsRegistry>,
        subgraph_hash: &str,
        stopwatch: StopwatchMetrics,
    ) -> Self {
        let block_trigger_count = registry
            .new_deployment_histogram(
                "deployment_block_trigger_count",
                "Measures the number of triggers in each block for a subgraph deployment",
                subgraph_hash,
                vec![1.0, 5.0, 10.0, 20.0, 50.0],
            )
            .expect("failed to create `deployment_block_trigger_count` histogram");
        let trigger_processing_duration = registry
            .new_deployment_histogram(
                "deployment_trigger_processing_duration",
                "Measures duration of trigger processing for a subgraph deployment",
                subgraph_hash,
                vec![0.01, 0.05, 0.1, 0.5, 1.5, 5.0, 10.0, 30.0, 120.0],
            )
            .expect("failed to create `deployment_trigger_processing_duration` histogram");
        let block_processing_duration = registry
            .new_deployment_histogram(
                "deployment_block_processing_duration",
                "Measures duration of block processing for a subgraph deployment",
                subgraph_hash,
                vec![0.05, 0.2, 0.7, 1.5, 4.0, 10.0, 60.0, 120.0, 240.0],
            )
            .expect("failed to create `deployment_block_processing_duration` histogram");
        let block_ops_transaction_duration = registry
            .new_deployment_histogram(
                "deployment_transact_block_operations_duration",
                "Measures duration of commiting all the entity operations in a block and updating the subgraph pointer",
                subgraph_hash,
                vec![0.01, 0.05, 0.1, 0.3, 0.7, 2.0],
            )
            .expect("failed to create `deployment_transact_block_operations_duration_{}");

        let firehose_connection_errors = registry
            .new_deployment_counter(
                "firehose_connection_errors",
                "Measures connections when trying to obtain a firehose connection",
                subgraph_hash,
            )
            .expect("failed to create firehose_connection_errors counter");

        Self {
            block_trigger_count,
            block_processing_duration,
            trigger_processing_duration,
            block_ops_transaction_duration,
            firehose_connection_errors,
            stopwatch,
        }
    }

    pub fn observe_trigger_processing_duration(&self, duration: f64) {
        self.trigger_processing_duration.observe(duration);
    }

    pub fn unregister(&self, registry: Arc<MetricsRegistry>) {
        registry.unregister(self.block_processing_duration.clone());
        registry.unregister(self.block_trigger_count.clone());
        registry.unregister(self.trigger_processing_duration.clone());
        registry.unregister(self.block_ops_transaction_duration.clone());
    }
}

#[derive(Debug)]
pub struct SubgraphCountMetric {
    pub running_count: Box<Gauge>,
    pub deployment_count: Box<Gauge>,
}

impl SubgraphCountMetric {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        let running_count = registry
            .new_gauge(
                "deployment_running_count",
                "Counts the number of deployments currently being indexed by the graph-node.",
                HashMap::new(),
            )
            .expect("failed to create `deployment_count` gauge");
        let deployment_count = registry
            .new_gauge(
                "deployment_count",
                "Counts the number of deployments currently deployed to the graph-node.",
                HashMap::new(),
            )
            .expect("failed to create `deployment_count` gauge");
        Self {
            running_count,
            deployment_count,
        }
    }
}

pub struct RunnerMetrics {
    /// Sensors to measure the execution of the subgraph instance
    pub subgraph: Arc<SubgraphInstanceMetrics>,
    /// Sensors to measure the execution of the subgraph's runtime hosts
    pub host: Arc<HostMetrics>,
    /// Sensors to measure the BlockStream metrics
    pub stream: Arc<BlockStreamMetrics>,
}
