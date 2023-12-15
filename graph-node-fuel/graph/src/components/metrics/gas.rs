use super::MetricsRegistry;
use crate::prelude::DeploymentHash;
use prometheus::CounterVec;
use std::sync::Arc;

#[derive(Clone)]
pub struct GasMetrics {
    pub gas_counter: CounterVec,
    pub op_counter: CounterVec,
}

impl GasMetrics {
    pub fn new(subgraph_id: DeploymentHash, registry: Arc<MetricsRegistry>) -> Self {
        let gas_counter = registry
            .global_deployment_counter_vec(
                "deployment_gas",
                "total gas used",
                subgraph_id.as_str(),
                &["method"],
            )
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to register deployment_gas prometheus counter for {}: {}",
                    subgraph_id, err
                )
            });

        let op_counter = registry
            .global_deployment_counter_vec(
                "deployment_op_count",
                "total number of operations",
                subgraph_id.as_str(),
                &["method"],
            )
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to register deployment_op_count prometheus counter for {}: {}",
                    subgraph_id, err
                )
            });

        GasMetrics {
            gas_counter,
            op_counter,
        }
    }

    pub fn mock() -> Self {
        let subgraph_id = DeploymentHash::default();
        Self::new(subgraph_id, Arc::new(MetricsRegistry::mock()))
    }

    pub fn track_gas(&self, method: &str, gas_used: u64) {
        self.gas_counter
            .with_label_values(&[method])
            .inc_by(gas_used as f64);
    }

    pub fn track_operations(&self, method: &str, op_count: u64) {
        self.op_counter
            .with_label_values(&[method])
            .inc_by(op_count as f64);
    }
}
