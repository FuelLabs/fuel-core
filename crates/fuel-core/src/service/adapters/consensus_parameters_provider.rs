use crate::{
    database::Database,
    service::adapters::BlockImporterAdapter,
};
use fuel_core_producer::ports::BlockProducerDatabase;
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    SharedMutex,
    StateWatcher,
};
use fuel_core_storage::{
    not_found,
    tables::ConsensusParametersVersions,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::ports::BlockImporter;
use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    fuel_tx::ConsensusParameters,
    services::block_importer::SharedImportResult,
};
use futures::StreamExt;
use std::{
    collections::HashMap,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct SharedState {
    latest_consensus_parameters_version: Arc<SharedMutex<ConsensusParametersVersion>>,
    consensus_parameters:
        Arc<SharedMutex<HashMap<ConsensusParametersVersion, Arc<ConsensusParameters>>>>,
    database: Database,
}

pub struct Task {
    blocks_events: BoxStream<SharedImportResult>,
    shared_state: SharedState,
}

impl SharedState {
    fn new(database: Database) -> StorageResult<Self> {
        let genesis_version = 0;
        let state = Self {
            latest_consensus_parameters_version: Arc::new(SharedMutex::new(
                genesis_version,
            )),
            consensus_parameters: Default::default(),
            database,
        };

        Ok(state)
    }

    fn cache_consensus_parameters(
        &self,
        version: ConsensusParametersVersion,
    ) -> StorageResult<Arc<ConsensusParameters>> {
        let consensus_parameters = self
            .database
            .storage::<ConsensusParametersVersions>()
            .get(&version)?
            .ok_or(not_found!(ConsensusParametersVersions))?
            .into_owned();

        let consensus_parameters = Arc::new(consensus_parameters);
        self.consensus_parameters
            .lock()
            .insert(version, consensus_parameters.clone());
        Ok(consensus_parameters)
    }

    pub fn get_consensus_parameters(
        &self,
        version: &ConsensusParametersVersion,
    ) -> StorageResult<Arc<ConsensusParameters>> {
        {
            let consensus_parameters = self.consensus_parameters.lock();
            if let Some(parameters) = consensus_parameters.get(version) {
                return Ok(parameters.clone());
            }
        }

        self.cache_consensus_parameters(*version)
    }

    pub fn latest_consensus_parameters(&self) -> Arc<ConsensusParameters> {
        let version = *self.latest_consensus_parameters_version.lock();
        self.get_consensus_parameters(&version)
            .expect("The latest consensus parameters always are available unless this function was called before regenesis.")
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {

            _ = watcher.while_started() => {
                should_continue = false;
            }

            Some(event) = self.blocks_events.next() => {
                let new_version = event
                    .sealed_block
                    .entity
                    .header()
                    .application()
                    .consensus_parameters_version;

                if new_version > *self.shared_state.latest_consensus_parameters_version.lock() {
                    match self.shared_state.cache_consensus_parameters(new_version) {
                        Ok(_) => {
                            *self.shared_state.latest_consensus_parameters_version.lock() = new_version;
                        }
                        Err(err) => {
                            tracing::error!("Failed to cache consensus parameters: {:?}", err);
                            should_continue = false;
                            return Ok(should_continue)
                        }
                    }
                }
                should_continue = true;
            }
        }

        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // We don't have any resources to clean up.
        Ok(())
    }
}

#[async_trait::async_trait]
impl RunnableService for Task {
    const NAME: &'static str = "ConsensusParametersProviderTask";
    type SharedData = SharedState;
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let latest_consensus_parameters_version = self
            .shared_state
            .database
            .latest_consensus_parameters_version()?;
        self.shared_state
            .cache_consensus_parameters(latest_consensus_parameters_version)?;
        *self.shared_state.latest_consensus_parameters_version.lock() =
            latest_consensus_parameters_version;

        Ok(self)
    }
}

pub fn new_service(
    database: Database,
    importer: &BlockImporterAdapter,
) -> StorageResult<ServiceRunner<Task>> {
    let blocks_events = importer.block_events();
    Ok(ServiceRunner::new(Task {
        blocks_events,
        shared_state: SharedState::new(database)?,
    }))
}
