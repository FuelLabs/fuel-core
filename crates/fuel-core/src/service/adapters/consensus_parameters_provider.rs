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
    transactional::AtomicView,
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
    fmt::Debug,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct SharedState {
    pub(crate) latest_consensus_parameters_version:
        SharedMutex<ConsensusParametersVersion>,
    pub(crate) consensus_parameters:
        SharedMutex<HashMap<ConsensusParametersVersion, Arc<ConsensusParameters>>>,
    pub(crate) database: Database,
}

pub struct Task {
    blocks_events: BoxStream<SharedImportResult>,
    shared_state: SharedState,
}

impl Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("shared_state", &self.shared_state)
            .finish()
    }
}

impl SharedState {
    fn new(database: Database) -> Self {
        let genesis_version = 0;
        Self {
            latest_consensus_parameters_version: SharedMutex::new(genesis_version),
            consensus_parameters: Default::default(),
            database,
        }
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
        self.latest_consensus_parameters_with_version().1
    }

    pub fn latest_consensus_parameters_with_version(
        &self,
    ) -> (ConsensusParametersVersion, Arc<ConsensusParameters>) {
        let version = *self.latest_consensus_parameters_version.lock();
        let params = self.get_consensus_parameters(&version)
            .expect("The latest consensus parameters always are available unless this function was called before regenesis.");
        (version, params)
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            biased;

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
            .latest_view()?
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
) -> ServiceRunner<Task> {
    let blocks_events = importer.block_events();
    ServiceRunner::new(Task {
        blocks_events,
        shared_state: SharedState::new(database),
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        database::Database,
        service::adapters::consensus_parameters_provider::{
            SharedState,
            Task,
        },
    };
    use fuel_core_services::{
        stream::IntoBoxStream,
        RunnableService,
        RunnableTask,
        StateWatcher,
    };
    use fuel_core_storage::{
        tables::ConsensusParametersVersions,
        transactional::IntoTransaction,
        StorageAsMut,
    };
    use fuel_core_types::{
        blockchain::{
            block::Block,
            header::ConsensusParametersVersion,
            SealedBlock,
        },
        fuel_tx::ConsensusParameters,
        services::block_importer::{
            ImportResult,
            SharedImportResult,
        },
    };
    use futures::stream;

    fn add_consensus_parameters(
        database: Database,
        consensus_parameters_version: ConsensusParametersVersion,
        consensus_parameters: &ConsensusParameters,
    ) -> Database {
        let mut database = database.into_transaction();
        database
            .storage_as_mut::<ConsensusParametersVersions>()
            .insert(&consensus_parameters_version, consensus_parameters)
            .unwrap();
        database.commit().unwrap()
    }

    #[tokio::test]
    async fn latest_consensus_parameters_works() {
        let version = 0;
        let consensus_parameters = Default::default();

        // Given
        let database =
            add_consensus_parameters(Database::default(), version, &consensus_parameters);
        let state = SharedState::new(database);

        // When
        let fetched_parameters = state.latest_consensus_parameters();

        // Then
        assert_eq!(fetched_parameters.as_ref(), &consensus_parameters);
    }

    #[tokio::test]
    async fn into_task_works_with_non_empty_database() {
        let version = 0;
        let consensus_parameters = Default::default();

        // Given
        let non_empty_database =
            add_consensus_parameters(Database::default(), version, &consensus_parameters);
        let task = Task {
            blocks_events: stream::empty().into_boxed(),
            shared_state: SharedState::new(non_empty_database),
        };

        // When
        let result = task.into_task(&Default::default(), ()).await;

        // Then
        result.expect("Initialization should succeed because database contains consensus parameters.");
    }

    #[tokio::test]
    async fn into_task_fails_when_no_parameters() {
        // Given
        let empty_database = Database::default();
        let task = Task {
            blocks_events: stream::empty().into_boxed(),
            shared_state: SharedState::new(empty_database),
        };

        // When
        let result = task.into_task(&Default::default(), ()).await;

        // Then
        result.expect_err(
            "Initialization should fails because of lack of consensus parameters",
        );
    }

    fn result_with_new_version(
        version: ConsensusParametersVersion,
    ) -> SharedImportResult {
        let mut block = Block::default();
        block
            .header_mut()
            .application_mut()
            .consensus_parameters_version = version;
        let sealed_block = SealedBlock {
            entity: block,
            consensus: Default::default(),
        };
        std::sync::Arc::new(ImportResult::new_from_local(
            sealed_block,
            Default::default(),
            Default::default(),
        ))
    }

    #[tokio::test]
    async fn run_updates_the_latest_consensus_parameters_from_imported_block() {
        use futures::StreamExt;

        let old_version = 0;
        let new_version = 1234;
        let old_consensus_parameters = ConsensusParameters::default();
        let mut new_consensus_parameters = ConsensusParameters::default();
        new_consensus_parameters.set_privileged_address([123; 32].into());
        assert_ne!(old_consensus_parameters, new_consensus_parameters);

        let (block_sender, block_receiver) =
            tokio::sync::broadcast::channel::<SharedImportResult>(1);
        let database_with_old_parameters = add_consensus_parameters(
            Database::default(),
            old_version,
            &old_consensus_parameters,
        );
        let mut task = Task {
            blocks_events: IntoBoxStream::into_boxed(
                tokio_stream::wrappers::BroadcastStream::new(block_receiver)
                    .filter_map(|r| futures::future::ready(r.ok())),
            ),
            shared_state: SharedState::new(database_with_old_parameters.clone()),
        }
        .into_task(&Default::default(), ())
        .await
        .unwrap();
        assert_eq!(
            task.shared_state.latest_consensus_parameters().as_ref(),
            &old_consensus_parameters
        );

        // Given
        add_consensus_parameters(
            database_with_old_parameters,
            new_version,
            &new_consensus_parameters,
        );

        // When
        let result_with_new_version = result_with_new_version(new_version);
        let _ = block_sender.send(result_with_new_version);
        task.run(&mut StateWatcher::started()).await.unwrap();

        // Then
        assert_eq!(
            task.shared_state.latest_consensus_parameters().as_ref(),
            &new_consensus_parameters
        );
    }
}
