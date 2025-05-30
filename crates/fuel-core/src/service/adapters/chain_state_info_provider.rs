use crate::{
    database::Database,
    service::adapters::BlockImporterAdapter,
};
use fuel_core_producer::ports::BlockProducerDatabase;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    SharedRwLock,
    StateWatcher,
    TaskNextAction,
    stream::BoxStream,
};
use fuel_core_storage::{
    Result as StorageResult,
    StorageAsRef,
    not_found,
    tables::ConsensusParametersVersions,
    transactional::AtomicView,
};
use fuel_core_txpool::ports::BlockImporter;
use fuel_core_types::{
    blockchain::header::{
        ConsensusParametersVersion,
        StateTransitionBytecodeVersion,
    },
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
        SharedRwLock<ConsensusParametersVersion>,
    pub(crate) consensus_parameters:
        SharedRwLock<HashMap<ConsensusParametersVersion, Arc<ConsensusParameters>>>,
    pub(crate) latest_stf_version: SharedRwLock<StateTransitionBytecodeVersion>,
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
        // We set versions in the `into_task` function during start of the service.
        let genesis_version = 0;
        let genesis_stf_version = 0;

        Self {
            latest_consensus_parameters_version: SharedRwLock::new(genesis_version),
            latest_stf_version: SharedRwLock::new(genesis_stf_version),
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
            .write()
            .insert(version, consensus_parameters.clone());
        Ok(consensus_parameters)
    }

    pub fn get_consensus_parameters(
        &self,
        version: &ConsensusParametersVersion,
    ) -> StorageResult<Arc<ConsensusParameters>> {
        {
            let consensus_parameters = self.consensus_parameters.read();
            if let Some(parameters) = consensus_parameters.get(version) {
                return Ok(parameters.clone());
            }
        }

        self.cache_consensus_parameters(*version)
    }

    fn cache_consensus_parameters_version(&self, version: ConsensusParametersVersion) {
        *self.latest_consensus_parameters_version.write() = version;
    }

    pub fn latest_consensus_parameters(&self) -> Arc<ConsensusParameters> {
        self.latest_consensus_parameters_with_version().1
    }

    pub fn latest_consensus_parameters_version(&self) -> ConsensusParametersVersion {
        *self.latest_consensus_parameters_version.read()
    }

    pub fn latest_consensus_parameters_with_version(
        &self,
    ) -> (ConsensusParametersVersion, Arc<ConsensusParameters>) {
        let version = self.latest_consensus_parameters_version();
        let params = self.get_consensus_parameters(&version)
            .expect("The latest consensus parameters always are available unless this function was called before regenesis.");
        (version, params)
    }

    fn cache_stf_version(&self, version: StateTransitionBytecodeVersion) {
        *self.latest_stf_version.write() = version;
    }

    pub fn latest_stf_version(&self) -> StateTransitionBytecodeVersion {
        *self.latest_stf_version.read()
    }
}

impl RunnableTask for Task {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }

            Some(event) = self.blocks_events.next() => {
                let header = event
                    .sealed_block
                    .entity
                    .header();

                let new_consensus_parameters_version = header.consensus_parameters_version();
                if new_consensus_parameters_version > self.shared_state.latest_consensus_parameters_version() {
                    match self.shared_state.cache_consensus_parameters(new_consensus_parameters_version) {
                        Ok(_) => {
                            self.shared_state.cache_consensus_parameters_version(new_consensus_parameters_version);
                        }
                        Err(err) => {
                            tracing::error!("Failed to cache consensus parameters: {:?}", err);
                            return TaskNextAction::Stop
                        }
                    }
                }

                let new_stf_version = header.state_transition_bytecode_version();
                if new_stf_version > self.shared_state.latest_stf_version() {
                    self.shared_state.cache_stf_version(new_stf_version);
                }

                TaskNextAction::Continue
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // We don't have any resources to clean up.
        Ok(())
    }
}

#[async_trait::async_trait]
impl RunnableService for Task {
    const NAME: &'static str = "ChainStateInfoProviderTask";
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
        let latest_stf_version = self
            .shared_state
            .database
            .latest_view()?
            .latest_state_transition_bytecode_version()?;

        self.shared_state
            .cache_consensus_parameters_version(latest_consensus_parameters_version);
        self.shared_state
            .cache_consensus_parameters(latest_consensus_parameters_version)?;
        self.shared_state.cache_stf_version(latest_stf_version);

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
        service::adapters::chain_state_info_provider::{
            SharedState,
            Task,
        },
    };
    use fuel_core_services::{
        RunnableService,
        RunnableTask,
        StateWatcher,
        stream::IntoBoxStream,
    };
    use fuel_core_storage::{
        StorageAsMut,
        tables::{
            ConsensusParametersVersions,
            StateTransitionBytecodeVersions,
        },
        transactional::IntoTransaction,
    };
    use fuel_core_types::{
        blockchain::{
            SealedBlock,
            block::Block,
            header::{
                ConsensusParametersVersion,
                StateTransitionBytecodeVersion,
            },
        },
        fuel_tx::{
            Bytes32,
            ConsensusParameters,
        },
        services::block_importer::{
            ImportResult,
            SharedImportResult,
        },
    };
    use futures::stream;

    fn add_consensus_parameters_and_stf_version(
        database: Database,
        consensus_parameters_version: ConsensusParametersVersion,
        consensus_parameters: &ConsensusParameters,
        stf_version: StateTransitionBytecodeVersion,
    ) -> Database {
        let mut database = database.into_transaction();
        database
            .storage_as_mut::<ConsensusParametersVersions>()
            .insert(&consensus_parameters_version, consensus_parameters)
            .unwrap();

        const BYTECODE_ROOT: Bytes32 = Bytes32::zeroed();
        database
            .storage_as_mut::<StateTransitionBytecodeVersions>()
            .insert(&stf_version, &BYTECODE_ROOT)
            .unwrap();
        database.commit().unwrap()
    }

    #[tokio::test]
    async fn latest_consensus_parameters_works() {
        let consensus_parameters_version = 0;
        let consensus_parameters = Default::default();
        let stf_version = Default::default();

        // Given
        let database = add_consensus_parameters_and_stf_version(
            Database::default(),
            consensus_parameters_version,
            &consensus_parameters,
            stf_version,
        );
        let state = SharedState::new(database);

        // When
        let fetched_parameters = state.latest_consensus_parameters();

        // Then
        assert_eq!(fetched_parameters.as_ref(), &consensus_parameters);
    }

    #[tokio::test]
    async fn into_task_works_with_non_empty_database() {
        let consensus_parameters_version = 0;
        let consensus_parameters = Default::default();
        let stf_version = Default::default();

        // Given
        let non_empty_database = add_consensus_parameters_and_stf_version(
            Database::default(),
            consensus_parameters_version,
            &consensus_parameters,
            stf_version,
        );
        let task = Task {
            blocks_events: stream::empty().into_boxed(),
            shared_state: SharedState::new(non_empty_database),
        };

        // When
        let result = task.into_task(&Default::default(), ()).await;

        // Then
        result.expect("Initialization should succeed because database contains consensus parameters and stf version.");
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
            "Initialization should fails because of lack of consensus parameters and stf version",
        );
    }

    fn result_with_new_consensus_parameters_version(
        version: ConsensusParametersVersion,
    ) -> SharedImportResult {
        let mut block = Block::default();
        block.header_mut().set_consensus_parameters_version(version);
        let sealed_block = SealedBlock {
            entity: block,
            consensus: Default::default(),
        };
        std::sync::Arc::new(
            ImportResult::new_from_local(
                sealed_block,
                Default::default(),
                Default::default(),
            )
            .wrap(),
        )
    }

    #[tokio::test]
    async fn run_updates_the_latest_consensus_parameters_from_imported_block() {
        use futures::StreamExt;

        let stf_version = Default::default();

        let old_consensus_parameters_version = 0;
        let new_consensus_parameters_version = 1234;
        let old_consensus_parameters = ConsensusParameters::default();
        let mut new_consensus_parameters = ConsensusParameters::default();
        new_consensus_parameters.set_privileged_address([123; 32].into());
        assert_ne!(old_consensus_parameters, new_consensus_parameters);

        let (block_sender, block_receiver) =
            tokio::sync::broadcast::channel::<SharedImportResult>(1);
        let database_with_old_parameters = add_consensus_parameters_and_stf_version(
            Database::default(),
            old_consensus_parameters_version,
            &old_consensus_parameters,
            stf_version,
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
        add_consensus_parameters_and_stf_version(
            database_with_old_parameters,
            new_consensus_parameters_version,
            &new_consensus_parameters,
            stf_version,
        );

        // When
        let result_with_new_version = result_with_new_consensus_parameters_version(
            new_consensus_parameters_version,
        );
        let _ = block_sender.send(result_with_new_version);
        let _ = task.run(&mut StateWatcher::started()).await;

        // Then
        assert_eq!(
            task.shared_state.latest_consensus_parameters().as_ref(),
            &new_consensus_parameters
        );
    }

    fn result_with_new_stf_version(
        version: StateTransitionBytecodeVersion,
    ) -> SharedImportResult {
        let mut block = Block::default();
        block.header_mut().set_stf_version(version);
        let sealed_block = SealedBlock {
            entity: block,
            consensus: Default::default(),
        };
        std::sync::Arc::new(
            ImportResult::new_from_local(
                sealed_block,
                Default::default(),
                Default::default(),
            )
            .wrap(),
        )
    }

    #[tokio::test]
    async fn run_updates_the_latest_stf_version_from_imported_block() {
        use futures::StreamExt;

        let old_stf_version = 0;
        let new_stf_version = 1234;

        let consensus_parameters_version = 0;
        let consensus_parameters = ConsensusParameters::default();

        let (block_sender, block_receiver) =
            tokio::sync::broadcast::channel::<SharedImportResult>(1);
        let database_with_old_parameters = add_consensus_parameters_and_stf_version(
            Database::default(),
            consensus_parameters_version,
            &consensus_parameters,
            old_stf_version,
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
        assert_eq!(task.shared_state.latest_stf_version(), old_stf_version);

        // Given
        add_consensus_parameters_and_stf_version(
            database_with_old_parameters,
            consensus_parameters_version,
            &consensus_parameters,
            new_stf_version,
        );

        // When
        let result_with_new_version = result_with_new_stf_version(new_stf_version);
        let _ = block_sender.send(result_with_new_version);
        let _ = task.run(&mut StateWatcher::started()).await;

        // Then
        assert_eq!(task.shared_state.latest_stf_version(), new_stf_version);
    }
}
