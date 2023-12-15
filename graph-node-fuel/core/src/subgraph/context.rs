pub mod instance;

use crate::polling_monitor::{
    spawn_monitor, ArweaveService, IpfsService, PollingMonitor, PollingMonitorMetrics,
};
use anyhow::{self, Error};
use bytes::Bytes;
use graph::{
    blockchain::Blockchain,
    components::{
        store::{DeploymentId, SubgraphFork},
        subgraph::{MappingError, SharedProofOfIndexing},
    },
    data_source::{
        offchain::{self, Base64},
        CausalityRegion, DataSource, TriggerData,
    },
    ipfs_client::CidFile,
    prelude::{
        BlockNumber, BlockPtr, BlockState, CancelGuard, CheapClone, DeploymentHash,
        MetricsRegistry, RuntimeHost, RuntimeHostBuilder, SubgraphCountMetric,
        SubgraphInstanceMetrics, TriggerProcessor,
    },
    slog::Logger,
    tokio::sync::mpsc,
};
use std::sync::{Arc, RwLock};
use std::{collections::HashMap, time::Instant};

use self::instance::SubgraphInstance;

#[derive(Clone, Debug)]
pub struct SubgraphKeepAlive {
    alive_map: Arc<RwLock<HashMap<DeploymentId, CancelGuard>>>,
    sg_metrics: Arc<SubgraphCountMetric>,
}

impl CheapClone for SubgraphKeepAlive {}

impl SubgraphKeepAlive {
    pub fn new(sg_metrics: Arc<SubgraphCountMetric>) -> Self {
        Self {
            sg_metrics,
            alive_map: Arc::new(RwLock::new(HashMap::default())),
        }
    }

    pub fn remove(&self, deployment_id: &DeploymentId) {
        self.alive_map.write().unwrap().remove(deployment_id);
        self.sg_metrics.running_count.dec();
    }
    pub fn insert(&self, deployment_id: DeploymentId, guard: CancelGuard) {
        self.alive_map.write().unwrap().insert(deployment_id, guard);
        self.sg_metrics.running_count.inc();
    }
}

// The context keeps track of mutable in-memory state that is retained across blocks.
//
// Currently most of the changes are applied in `runner.rs`, but ideally more of that would be
// refactored into the context so it wouldn't need `pub` fields. The entity cache should probably
// also be moved here.
pub struct IndexingContext<C, T>
where
    T: RuntimeHostBuilder<C>,
    C: Blockchain,
{
    instance: SubgraphInstance<C, T>,
    pub instances: SubgraphKeepAlive,
    pub offchain_monitor: OffchainMonitor,
    pub filter: Option<C::TriggerFilter>,
    trigger_processor: Box<dyn TriggerProcessor<C, T>>,
}

impl<C: Blockchain, T: RuntimeHostBuilder<C>> IndexingContext<C, T> {
    pub fn new(
        instance: SubgraphInstance<C, T>,
        instances: SubgraphKeepAlive,
        offchain_monitor: OffchainMonitor,
        trigger_processor: Box<dyn TriggerProcessor<C, T>>,
    ) -> Self {
        Self {
            instance,
            instances,
            offchain_monitor,
            filter: None,
            trigger_processor,
        }
    }

    pub async fn process_trigger(
        &self,
        logger: &Logger,
        block: &Arc<C::Block>,
        trigger: &TriggerData<C>,
        state: BlockState<C>,
        proof_of_indexing: &SharedProofOfIndexing,
        causality_region: &str,
        debug_fork: &Option<Arc<dyn SubgraphFork>>,
        subgraph_metrics: &Arc<SubgraphInstanceMetrics>,
        instrument: bool,
    ) -> Result<BlockState<C>, MappingError> {
        self.process_trigger_in_hosts(
            logger,
            self.instance.hosts_for_trigger(trigger),
            block,
            trigger,
            state,
            proof_of_indexing,
            causality_region,
            debug_fork,
            subgraph_metrics,
            instrument,
        )
        .await
    }

    pub async fn process_block(
        &self,
        logger: &Logger,
        block_ptr: BlockPtr,
        block_data: Box<[u8]>,
        handler: String,
        mut state: BlockState<C>,
        proof_of_indexing: &SharedProofOfIndexing,
        causality_region: &str,
        debug_fork: &Option<Arc<dyn SubgraphFork>>,
        subgraph_metrics: &Arc<SubgraphInstanceMetrics>,
        instrument: bool,
    ) -> Result<BlockState<C>, MappingError> {
        let error_count = state.deterministic_errors.len();

        if let Some(proof_of_indexing) = proof_of_indexing {
            proof_of_indexing
                .borrow_mut()
                .start_handler(causality_region);
        }

        let start = Instant::now();

        // This flow is expected to have a single data source(and a corresponding host) which
        // gets executed every block.
        state = self
            .instance
            .hosts()
            .first()
            .expect("Expected this flow to have exactly one host")
            .process_block(
                logger,
                block_ptr,
                block_data,
                handler,
                state,
                proof_of_indexing.cheap_clone(),
                debug_fork,
                instrument,
            )
            .await?;

        let elapsed = start.elapsed().as_secs_f64();
        subgraph_metrics.observe_trigger_processing_duration(elapsed);

        if let Some(proof_of_indexing) = proof_of_indexing {
            if state.deterministic_errors.len() != error_count {
                assert!(state.deterministic_errors.len() == error_count + 1);

                // If a deterministic error has happened, write a new
                // ProofOfIndexingEvent::DeterministicError to the SharedProofOfIndexing.
                proof_of_indexing
                    .borrow_mut()
                    .write_deterministic_error(logger, causality_region);
            }
        }

        Ok(state)
    }
    pub async fn process_trigger_in_hosts(
        &self,
        logger: &Logger,
        hosts: Box<dyn Iterator<Item = &T::Host> + Send + '_>,
        block: &Arc<C::Block>,
        trigger: &TriggerData<C>,
        state: BlockState<C>,
        proof_of_indexing: &SharedProofOfIndexing,
        causality_region: &str,
        debug_fork: &Option<Arc<dyn SubgraphFork>>,
        subgraph_metrics: &Arc<SubgraphInstanceMetrics>,
        instrument: bool,
    ) -> Result<BlockState<C>, MappingError> {
        self.trigger_processor
            .process_trigger(
                logger,
                hosts,
                block,
                trigger,
                state,
                proof_of_indexing,
                causality_region,
                debug_fork,
                subgraph_metrics,
                instrument,
            )
            .await
    }

    /// Removes data sources hosts with a creation block greater or equal to `reverted_block`, so
    /// that they are no longer candidates for `process_trigger`.
    ///
    /// This does not currently affect the `offchain_monitor` or the `filter`, so they will continue
    /// to include data sources that have been reverted. This is not ideal for performance, but it
    /// does not affect correctness since triggers that have no matching host will be ignored by
    /// `process_trigger`.
    ///
    /// File data sources that have been marked not done during this process will get re-queued
    pub fn revert_data_sources(&mut self, reverted_block: BlockNumber) -> Result<(), Error> {
        let removed = self.instance.revert_data_sources(reverted_block);

        removed
            .into_iter()
            .try_for_each(|source| self.offchain_monitor.add_source(source))
    }

    pub fn add_dynamic_data_source(
        &mut self,
        logger: &Logger,
        data_source: DataSource<C>,
    ) -> Result<Option<Arc<T::Host>>, Error> {
        let source = data_source.as_offchain().map(|ds| ds.source.clone());
        let host = self.instance.add_dynamic_data_source(logger, data_source)?;

        if host.is_some() {
            if let Some(source) = source {
                self.offchain_monitor.add_source(source)?;
            }
        }

        Ok(host)
    }

    pub fn causality_region_next_value(&mut self) -> CausalityRegion {
        self.instance.causality_region_next_value()
    }

    pub fn instance(&self) -> &SubgraphInstance<C, T> {
        &self.instance
    }
}

pub struct OffchainMonitor {
    ipfs_monitor: PollingMonitor<CidFile>,
    ipfs_monitor_rx: mpsc::UnboundedReceiver<(CidFile, Bytes)>,
    arweave_monitor: PollingMonitor<Base64>,
    arweave_monitor_rx: mpsc::UnboundedReceiver<(Base64, Bytes)>,
}

impl OffchainMonitor {
    pub fn new(
        logger: Logger,
        registry: Arc<MetricsRegistry>,
        subgraph_hash: &DeploymentHash,
        ipfs_service: IpfsService,
        arweave_service: ArweaveService,
    ) -> Self {
        let metrics = Arc::new(PollingMonitorMetrics::new(registry, subgraph_hash));
        // The channel is unbounded, as it is expected that `fn ready_offchain_events` is called
        // frequently, or at least with the same frequency that requests are sent.
        let (ipfs_monitor_tx, ipfs_monitor_rx) = mpsc::unbounded_channel();
        let (arweave_monitor_tx, arweave_monitor_rx) = mpsc::unbounded_channel();

        let ipfs_monitor = spawn_monitor(
            ipfs_service,
            ipfs_monitor_tx,
            logger.cheap_clone(),
            metrics.cheap_clone(),
        );

        let arweave_monitor = spawn_monitor(arweave_service, arweave_monitor_tx, logger, metrics);
        Self {
            ipfs_monitor,
            ipfs_monitor_rx,
            arweave_monitor,
            arweave_monitor_rx,
        }
    }

    fn add_source(&mut self, source: offchain::Source) -> Result<(), Error> {
        match source {
            offchain::Source::Ipfs(cid_file) => self.ipfs_monitor.monitor(cid_file),
            offchain::Source::Arweave(base64) => self.arweave_monitor.monitor(base64),
        };
        Ok(())
    }

    pub fn ready_offchain_events(&mut self) -> Result<Vec<offchain::TriggerData>, Error> {
        use graph::tokio::sync::mpsc::error::TryRecvError;

        let mut triggers = vec![];
        loop {
            match self.ipfs_monitor_rx.try_recv() {
                Ok((cid_file, data)) => triggers.push(offchain::TriggerData {
                    source: offchain::Source::Ipfs(cid_file),
                    data: Arc::new(data),
                }),
                Err(TryRecvError::Disconnected) => {
                    anyhow::bail!("ipfs monitor unexpectedly terminated")
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        loop {
            match self.arweave_monitor_rx.try_recv() {
                Ok((base64, data)) => triggers.push(offchain::TriggerData {
                    source: offchain::Source::Arweave(base64),
                    data: Arc::new(data),
                }),
                Err(TryRecvError::Disconnected) => {
                    anyhow::bail!("arweave monitor unexpectedly terminated")
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        Ok(triggers)
    }
}
