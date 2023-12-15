use std::cmp::PartialEq;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Error;
use async_trait::async_trait;
use futures::sync::mpsc;

use crate::components::metrics::gas::GasMetrics;
use crate::components::store::SubgraphFork;
use crate::data_source::{
    DataSource, DataSourceTemplate, MappingTrigger, TriggerData, TriggerWithHandler,
};
use crate::prelude::*;
use crate::runtime::HostExportError;
use crate::{blockchain::Blockchain, components::subgraph::SharedProofOfIndexing};

#[derive(Debug)]
pub enum MappingError {
    /// A possible reorg was detected while running the mapping.
    PossibleReorg(anyhow::Error),
    Unknown(anyhow::Error),
}

impl From<anyhow::Error> for MappingError {
    fn from(e: anyhow::Error) -> Self {
        MappingError::Unknown(e)
    }
}

impl From<HostExportError> for MappingError {
    fn from(value: HostExportError) -> MappingError {
        match value {
            HostExportError::PossibleReorg(e) => MappingError::PossibleReorg(e.into()),
            HostExportError::Deterministic(e) | HostExportError::Unknown(e) => {
                MappingError::Unknown(e.into())
            }
        }
    }
}

impl MappingError {
    pub fn context(self, s: String) -> Self {
        use MappingError::*;
        match self {
            PossibleReorg(e) => PossibleReorg(e.context(s)),
            Unknown(e) => Unknown(e.context(s)),
        }
    }
}

/// Common trait for runtime host implementations.
#[async_trait]
pub trait RuntimeHost<C: Blockchain>: Send + Sync + 'static {
    fn data_source(&self) -> &DataSource<C>;

    fn match_and_decode(
        &self,
        trigger: &TriggerData<C>,
        block: &Arc<C::Block>,
        logger: &Logger,
    ) -> Result<Option<TriggerWithHandler<MappingTrigger<C>>>, Error>;

    async fn process_block(
        &self,
        logger: &Logger,
        block_ptr: BlockPtr,
        block_data: Box<[u8]>,
        handler: String,
        state: BlockState<C>,
        proof_of_indexing: SharedProofOfIndexing,
        debug_fork: &Option<Arc<dyn SubgraphFork>>,
        instrument: bool,
    ) -> Result<BlockState<C>, MappingError>;

    async fn process_mapping_trigger(
        &self,
        logger: &Logger,
        block_ptr: BlockPtr,
        trigger: TriggerWithHandler<MappingTrigger<C>>,
        state: BlockState<C>,
        proof_of_indexing: SharedProofOfIndexing,
        debug_fork: &Option<Arc<dyn SubgraphFork>>,
        instrument: bool,
    ) -> Result<BlockState<C>, MappingError>;

    /// Block number in which this host was created.
    /// Returns `None` for static data sources.
    fn creation_block_number(&self) -> Option<BlockNumber>;

    /// Offchain data sources track done_at which is set once the
    /// trigger has been processed.
    fn done_at(&self) -> Option<BlockNumber>;

    /// Convenience function to avoid leaking internal representation of
    /// mutable number. Calling this on OnChain Datasources is a noop.
    fn set_done_at(&self, block: Option<BlockNumber>);
}

pub struct HostMetrics {
    handler_execution_time: Box<HistogramVec>,
    host_fn_execution_time: Box<HistogramVec>,
    eth_call_execution_time: Box<HistogramVec>,
    pub gas_metrics: GasMetrics,
    pub stopwatch: StopwatchMetrics,
}

impl HostMetrics {
    pub fn new(
        registry: Arc<MetricsRegistry>,
        subgraph: &str,
        stopwatch: StopwatchMetrics,
        gas_metrics: GasMetrics,
    ) -> Self {
        let handler_execution_time = registry
            .new_deployment_histogram_vec(
                "deployment_handler_execution_time",
                "Measures the execution time for handlers",
                subgraph,
                vec![String::from("handler")],
                vec![0.1, 0.5, 1.0, 10.0, 100.0],
            )
            .expect("failed to create `deployment_handler_execution_time` histogram");
        let eth_call_execution_time = registry
            .new_deployment_histogram_vec(
                "deployment_eth_call_execution_time",
                "Measures the execution time for eth_call",
                subgraph,
                vec![
                    String::from("contract_name"),
                    String::from("address"),
                    String::from("method"),
                ],
                vec![0.1, 0.5, 1.0, 10.0, 100.0],
            )
            .expect("failed to create `deployment_eth_call_execution_time` histogram");

        let host_fn_execution_time = registry
            .new_deployment_histogram_vec(
                "deployment_host_fn_execution_time",
                "Measures the execution time for host functions",
                subgraph,
                vec![String::from("host_fn_name")],
                vec![0.025, 0.05, 0.2, 2.0, 8.0, 20.0],
            )
            .expect("failed to create `deployment_host_fn_execution_time` histogram");
        Self {
            handler_execution_time,
            host_fn_execution_time,
            stopwatch,
            gas_metrics,
            eth_call_execution_time,
        }
    }

    pub fn observe_handler_execution_time(&self, duration: f64, handler: &str) {
        self.handler_execution_time
            .with_label_values(&[handler][..])
            .observe(duration);
    }

    pub fn observe_host_fn_execution_time(&self, duration: f64, fn_name: &str) {
        self.host_fn_execution_time
            .with_label_values(&[fn_name][..])
            .observe(duration);
    }

    pub fn observe_eth_call_execution_time(
        &self,
        duration: f64,
        contract_name: &str,
        address: &str,
        method: &str,
    ) {
        self.eth_call_execution_time
            .with_label_values(&[contract_name, address, method][..])
            .observe(duration);
    }

    pub fn time_host_fn_execution_region(
        self: Arc<HostMetrics>,
        fn_name: &'static str,
    ) -> HostFnExecutionTimer {
        HostFnExecutionTimer {
            start: Instant::now(),
            metrics: self,
            fn_name,
        }
    }
}

#[must_use]
pub struct HostFnExecutionTimer {
    start: Instant,
    metrics: Arc<HostMetrics>,
    fn_name: &'static str,
}

impl Drop for HostFnExecutionTimer {
    fn drop(&mut self) {
        let elapsed = (Instant::now() - self.start).as_secs_f64();
        self.metrics
            .observe_host_fn_execution_time(elapsed, self.fn_name)
    }
}

pub trait RuntimeHostBuilder<C: Blockchain>: Clone + Send + Sync + 'static {
    type Host: RuntimeHost<C> + PartialEq;
    type Req: 'static + Send;

    /// Build a new runtime host for a subgraph data source.
    fn build(
        &self,
        network_name: String,
        subgraph_id: DeploymentHash,
        data_source: DataSource<C>,
        top_level_templates: Arc<Vec<DataSourceTemplate<C>>>,
        mapping_request_sender: mpsc::Sender<Self::Req>,
        metrics: Arc<HostMetrics>,
    ) -> Result<Self::Host, Error>;

    /// Spawn a mapping and return a channel for mapping requests. The sender should be able to be
    /// cached and shared among mappings that use the same wasm file.
    fn spawn_mapping(
        raw_module: &[u8],
        logger: Logger,
        subgraph_id: DeploymentHash,
        metrics: Arc<HostMetrics>,
    ) -> Result<mpsc::Sender<Self::Req>, anyhow::Error>;
}
