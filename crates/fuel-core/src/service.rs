use self::adapters::BlockImporterAdapter;
use crate::{
    combined_database::{
        CombinedDatabase,
        ShutdownListener,
    },
    database::Database,
    service::{
        adapters::{
            ExecutorAdapter,
            PoAAdapter,
        },
        sub_services::TxPoolSharedState,
    },
};
use fuel_core_chain_config::{
    ConsensusConfig,
    GenesisCommitment,
};
use fuel_core_poa::{
    ports::BlockImporter,
    verifier::verify_consensus,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    State,
    StateWatcher,
};
use fuel_core_storage::{
    not_found,
    tables::SealedBlockConsensus,
    transactional::{
        AtomicView,
        ReadTransaction,
    },
    IsNotFound,
    StorageAsMut,
};
use fuel_core_types::blockchain::consensus::Consensus;
use std::{
    net::SocketAddr,
    sync::Arc,
};

pub use config::{
    Config,
    DbType,
    RelayerConsensusConfig,
    VMConfig,
};
pub use fuel_core_services::Service as ServiceTrait;

pub mod adapters;
pub mod config;
pub mod genesis;
pub mod metrics;
mod query;
pub mod sub_services;
pub mod vm_pool;

#[derive(Clone)]
pub struct SharedState {
    /// The PoA adaptor around the shared state of the consensus module.
    pub poa_adapter: PoAAdapter,
    /// The transaction pool shared state.
    pub txpool_shared_state: TxPoolSharedState,
    /// The P2P network shared state.
    #[cfg(feature = "p2p")]
    pub network: Option<fuel_core_p2p::service::SharedState>,
    #[cfg(feature = "relayer")]
    /// The Relayer shared state.
    pub relayer: Option<
        fuel_core_relayer::SharedState<
            Database<crate::database::database_description::relayer::Relayer>,
        >,
    >,
    /// The GraphQL shared state.
    pub graph_ql: crate::fuel_core_graphql_api::api_service::SharedState,
    /// The underlying database.
    pub database: CombinedDatabase,
    /// Subscribe to new block production.
    pub block_importer: BlockImporterAdapter,
    /// The executor to validate blocks.
    pub executor: ExecutorAdapter,
    /// The config of the service.
    pub config: Config,
}

pub struct FuelService {
    /// The `ServiceRunner` used for `FuelService`.
    ///
    /// # Dev-note: The `FuelService` is already exposed as a public API and used by many crates.
    /// To provide a user-friendly API and avoid breaking many downstream crates, `ServiceRunner`
    /// is wrapped inside.
    runner: ServiceRunner<Task>,
    /// The shared state of the service
    pub sub_services: Arc<SubServices>,
    /// The shared state of the service
    pub shared: SharedState,
    /// The address bound by the system for serving the API
    pub bound_address: SocketAddr,
}

impl Drop for FuelService {
    fn drop(&mut self) {
        self.send_stop_signal();
    }
}

impl FuelService {
    /// Creates a `FuelService` instance from service config
    #[tracing::instrument(skip_all, fields(name = %config.name))]
    pub fn new<Shutdown>(
        mut database: CombinedDatabase,
        config: Config,
        shutdown_listener: &mut Shutdown,
    ) -> anyhow::Result<Self>
    where
        Shutdown: ShutdownListener,
    {
        let config = config.make_config_consistent();

        // initialize state
        tracing::info!("Initializing database");
        database.check_version()?;

        Self::make_database_compatible_with_config(
            &mut database,
            &config,
            shutdown_listener,
        )?;

        // initialize sub services
        tracing::info!("Initializing sub services");
        let (services, shared) = sub_services::init_sub_services(&config, database)?;

        let sub_services = Arc::new(services);
        let task = Task::new(sub_services.clone(), shared.clone())?;
        let runner = ServiceRunner::new(task);
        let bound_address = runner.shared.graph_ql.bound_address;

        Ok(FuelService {
            sub_services,
            bound_address,
            shared,
            runner,
        })
    }

    /// Creates and starts fuel node instance from service config
    pub async fn new_node(config: Config) -> anyhow::Result<Self> {
        // initialize database
        let combined_database =
            CombinedDatabase::from_config(&config.combined_db_config)?;

        Self::from_combined_database(combined_database, config).await
    }

    /// Creates and starts fuel node instance from service config and a pre-existing on-chain database
    pub async fn from_database(
        database: Database,
        config: Config,
    ) -> anyhow::Result<Self> {
        let combined_database = CombinedDatabase::new(
            database,
            Default::default(),
            Default::default(),
            Default::default(),
        );
        Self::from_combined_database(combined_database, config).await
    }

    /// Creates and starts fuel node instance from service config and a pre-existing combined database
    pub async fn from_combined_database(
        combined_database: CombinedDatabase,
        config: Config,
    ) -> anyhow::Result<Self> {
        let mut listener = crate::ShutdownListener::spawn();
        let service = Self::new(combined_database, config, &mut listener)?;
        let state = service.start_and_await().await?;

        if !state.started() {
            return Err(anyhow::anyhow!(
                "The state of the service is not started: {state:?}"
            ));
        }
        Ok(service)
    }

    #[cfg(feature = "relayer")]
    /// Wait for the Relayer to be in sync with
    /// the data availability layer.
    ///
    /// Yields until the relayer reaches a point where it
    /// considered up to date. Note that there's no guarantee
    /// the relayer will ever catch up to the da layer and
    /// may fall behind immediately after this future completes.
    ///
    /// The only guarantee is that if this future completes then
    /// the relayer did reach consistency with the da layer for
    /// some period of time.
    pub async fn await_relayer_synced(&self) -> anyhow::Result<()> {
        if let Some(relayer_handle) = &self.runner.shared.relayer {
            relayer_handle.await_synced().await?;
        }
        Ok(())
    }

    fn make_database_compatible_with_config<Shutdown>(
        combined_database: &mut CombinedDatabase,
        config: &Config,
        shutdown_listener: &mut Shutdown,
    ) -> anyhow::Result<()>
    where
        Shutdown: ShutdownListener,
    {
        let start_up_consensus_config = &config.snapshot_reader.chain_config().consensus;

        let mut found_override_height = None;
        match start_up_consensus_config {
            ConsensusConfig::PoA { .. } => {
                // We don't support overriding of the heights for PoA version 1.
            }
            ConsensusConfig::PoAV2(poa) => {
                let on_chain_view = combined_database.on_chain().latest_view()?;

                for override_height in poa.get_all_overrides().keys() {
                    let Some(current_height) = on_chain_view.maybe_latest_height()?
                    else {
                        // Database is empty, nothing to rollback
                        return Ok(());
                    };

                    if override_height > &current_height {
                        return Ok(());
                    }

                    let block_header = on_chain_view
                        .get_sealed_block_header(override_height)?
                        .ok_or(not_found!("SealedBlockHeader"))?;
                    let header = block_header.entity;
                    let seal = block_header.consensus;

                    if let Consensus::PoA(poa_seal) = seal {
                        let block_valid = verify_consensus(
                            start_up_consensus_config,
                            &header,
                            &poa_seal,
                        );

                        if !block_valid {
                            found_override_height = Some(override_height);
                        }
                    } else {
                        return Err(anyhow::anyhow!(
                            "The consensus at override height {override_height} is not PoA."
                        ));
                    };
                }
            }
        }

        if let Some(override_height) = found_override_height {
            let rollback_height = override_height.pred().ok_or(anyhow::anyhow!(
                "The override height is zero. \
                The override height should be greater than zero."
            ))?;
            tracing::warn!(
                "The consensus at override height {override_height} \
                does not match with the database. \
                Rollbacking the database to the height {rollback_height}"
            );
            combined_database.rollback_to(rollback_height, shutdown_listener)?;
        }

        Ok(())
    }

    fn override_chain_config_if_needed(&self) -> anyhow::Result<()> {
        let chain_config = self.shared.config.snapshot_reader.chain_config();
        let on_chain_view = self.shared.database.on_chain().latest_view()?;
        let chain_config_hash = chain_config.root()?.into();
        let mut initialized_genesis = on_chain_view.get_genesis()?;
        let genesis_chain_config_hash = initialized_genesis.chain_config_hash;

        if genesis_chain_config_hash != chain_config_hash {
            tracing::warn!(
                "The genesis chain config hash({genesis_chain_config_hash}) \
                is different from the current one({chain_config_hash}). \
                Updating the genesis consensus parameters."
            );

            let genesis_block_height =
                on_chain_view.genesis_height()?.ok_or(anyhow::anyhow!(
                    "The genesis block height is not found in the database \
                    during overriding the chain config hash."
                ))?;
            let mut database_tx = on_chain_view.read_transaction();

            initialized_genesis.chain_config_hash = chain_config_hash;
            database_tx
                .storage_as_mut::<SealedBlockConsensus>()
                .insert(
                    &genesis_block_height,
                    &Consensus::Genesis(initialized_genesis),
                )?;

            self.shared
                .database
                .on_chain()
                .data
                .commit_changes(Some(genesis_block_height), database_tx.into_changes())?;
        }

        Ok(())
    }

    async fn prepare_genesis(&self, watcher: &StateWatcher) -> anyhow::Result<()> {
        // check if chain is initialized
        if let Err(err) = self.shared.database.on_chain().latest_view()?.get_genesis() {
            if err.is_not_found() {
                let result = genesis::execute_genesis_block(
                    watcher.clone(),
                    &self.shared.config,
                    &self.shared.database,
                )
                .await?;

                self.shared.block_importer.commit_result(result).await?;
            }
        }

        self.override_chain_config_if_needed()
    }
}

impl FuelService {
    /// Start all sub services and await for them to start.
    pub async fn start_and_await(&self) -> anyhow::Result<State> {
        let watcher = self.runner.state_watcher();
        self.prepare_genesis(&watcher).await?;
        self.runner.start_and_await().await
    }

    /// Sends the stop signal to all sub services.
    pub fn send_stop_signal(&self) -> bool {
        self.runner.stop()
    }

    /// Awaits for all services to shutdown.
    pub async fn await_shutdown(&self) -> anyhow::Result<State> {
        self.runner.await_stop().await
    }

    /// Sends the stop signal to all sub services and awaits for all services to shutdown.
    pub async fn send_stop_signal_and_await_shutdown(&self) -> anyhow::Result<State> {
        self.runner.stop_and_await().await
    }

    pub fn state(&self) -> State {
        self.runner.state()
    }

    pub fn sub_services(&self) -> &SubServices {
        self.sub_services.as_ref()
    }
}

pub type SubServices = Vec<Box<dyn ServiceTrait + Send + Sync + 'static>>;

struct Task {
    /// The list of started sub services.
    services: Arc<SubServices>,
    /// The address bound by the system for serving the API
    pub shared: SharedState,
}

impl Task {
    /// Private inner method for initializing the fuel service task
    pub fn new(services: Arc<SubServices>, shared: SharedState) -> anyhow::Result<Task> {
        Ok(Task { services, shared })
    }
}

#[async_trait::async_trait]
impl RunnableService for Task {
    const NAME: &'static str = "FuelService";
    type SharedData = SharedState;
    type Task = Task;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared.clone()
    }

    async fn into_task(
        mut self,
        watcher: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let mut watcher = watcher.clone();
        for service in self.services.iter() {
            tokio::select! {
                _ = watcher.wait_stopping_or_stopped() => {
                    break;
                }
                result = service.start_and_await() => {
                    result?;
                }
            }
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    #[tracing::instrument(skip_all)]
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let mut stop_signals = vec![];
        for service in self.services.iter() {
            stop_signals.push(service.await_stop())
        }
        stop_signals.push(Box::pin(watcher.while_started()));

        let (result, _, _) = futures::future::select_all(stop_signals).await;

        if let Err(err) = result {
            tracing::error!("Got an error during listen for shutdown: {}", err);
        }

        // We received the stop signal from any of one source, so stop this service and
        // all sub-services.
        let should_continue = false;
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        for service in self.services.iter() {
            let result = service.stop_and_await().await;

            if let Err(err) = result {
                tracing::error!(
                    "Got and error during awaiting for stop of the service: {}",
                    err
                );
            }
        }
        Ok(())
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use crate::{
        service::{
            Config,
            FuelService,
        },
        ShutdownListener,
    };
    use fuel_core_services::State;
    use std::{
        thread::sleep,
        time::Duration,
    };

    #[tokio::test]
    async fn stop_sub_service_shutdown_all_services() {
        // The test verify that if we stop any of sub-services
        let mut i = 0;
        loop {
            let mut shutdown = ShutdownListener::spawn();
            let service =
                FuelService::new(Default::default(), Config::local_node(), &mut shutdown)
                    .unwrap();
            service.start_and_await().await.unwrap();
            sleep(Duration::from_secs(1));
            for service in service.sub_services() {
                assert_eq!(service.state(), State::Started);
            }

            if i < service.sub_services().len() {
                service.sub_services()[i].stop_and_await().await.unwrap();
                tokio::time::timeout(Duration::from_secs(5), service.await_shutdown())
                    .await
                    .expect("Failed to stop the service in reasonable period of time")
                    .expect("Failed to stop the service");
            } else {
                break;
            }
            i += 1;
        }

        // current services: graphql, graphql worker, txpool, PoA, gas price service
        #[allow(unused_mut)]
        let mut expected_services = 6;

        // Relayer service is disabled with `Config::local_node`.
        // #[cfg(feature = "relayer")]
        // {
        //     expected_services += 1;
        // }
        #[cfg(feature = "p2p")]
        {
            // p2p & sync
            expected_services += 2;
        }

        // # Dev-note: Update the `expected_services` when we add/remove a new/old service.
        assert_eq!(i, expected_services);
    }

    #[tokio::test]
    async fn stop_and_await___stops_all_services() {
        let mut shutdown = ShutdownListener::spawn();
        let service =
            FuelService::new(Default::default(), Config::local_node(), &mut shutdown)
                .unwrap();
        service.start_and_await().await.unwrap();
        let sub_services_watchers: Vec<_> = service
            .sub_services()
            .iter()
            .map(|s| s.state_watcher())
            .collect();

        sleep(Duration::from_secs(1));
        for service in service.sub_services() {
            assert_eq!(service.state(), State::Started);
        }
        service.send_stop_signal_and_await_shutdown().await.unwrap();

        for mut service in sub_services_watchers {
            // Check that the state is `Stopped`(not `StoppedWithError`)
            assert_eq!(service.borrow_and_update().clone(), State::Stopped);
        }
    }
}
