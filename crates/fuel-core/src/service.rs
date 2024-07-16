use self::adapters::BlockImporterAdapter;
use crate::{
    combined_database::CombinedDatabase,
    database::Database,
    service::{
        adapters::{
            ExecutorAdapter,
            PoAAdapter,
        },
        sub_services::TxPoolSharedState,
    },
};
use fuel_core_poa::ports::BlockImporter;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    State,
    StateWatcher,
};
use fuel_core_storage::{
    transactional::AtomicView,
    IsNotFound,
};
use std::net::SocketAddr;

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
    pub shared: SharedState,
    /// The address bound by the system for serving the API
    pub bound_address: SocketAddr,
}

impl Drop for FuelService {
    fn drop(&mut self) {
        self.stop();
    }
}

impl FuelService {
    /// Creates a `FuelService` instance from service config
    #[tracing::instrument(skip_all, fields(name = %config.name))]
    pub fn new(database: CombinedDatabase, config: Config) -> anyhow::Result<Self> {
        let config = config.make_config_consistent();
        let task = Task::new(database, config)?;
        let runner = ServiceRunner::new(task);
        let shared = runner.shared.clone();
        let bound_address = runner.shared.graph_ql.bound_address;

        Ok(FuelService {
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
        let combined_database =
            CombinedDatabase::new(database, Default::default(), Default::default());
        Self::from_combined_database(combined_database, config).await
    }

    /// Creates and starts fuel node instance from service config and a pre-existing combined database
    pub async fn from_combined_database(
        combined_database: CombinedDatabase,
        config: Config,
    ) -> anyhow::Result<Self> {
        let service = Self::new(combined_database, config)?;
        let state = service.runner.start_and_await().await?;

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
}

#[async_trait::async_trait]
impl ServiceTrait for FuelService {
    fn start(&self) -> anyhow::Result<()> {
        self.runner.start()
    }

    async fn start_and_await(&self) -> anyhow::Result<State> {
        self.runner.start_and_await().await
    }

    async fn await_start_or_stop(&self) -> anyhow::Result<State> {
        self.runner.await_start_or_stop().await
    }

    fn stop(&self) -> bool {
        self.runner.stop()
    }

    async fn stop_and_await(&self) -> anyhow::Result<State> {
        self.runner.stop_and_await().await
    }

    async fn await_stop(&self) -> anyhow::Result<State> {
        self.runner.await_stop().await
    }

    fn state(&self) -> State {
        self.runner.state()
    }

    fn state_watcher(&self) -> StateWatcher {
        self.runner.state_watcher()
    }
}

pub type SubServices = Vec<Box<dyn ServiceTrait + Send + Sync + 'static>>;

pub struct Task {
    /// The list of started sub services.
    services: SubServices,
    /// The address bound by the system for serving the API
    pub shared: SharedState,
}

impl Task {
    /// Private inner method for initializing the fuel service task
    pub fn new(database: CombinedDatabase, config: Config) -> anyhow::Result<Task> {
        // initialize state
        tracing::info!("Initializing database");
        database.check_version()?;

        // initialize sub services
        tracing::info!("Initializing sub services");
        let (services, shared) = sub_services::init_sub_services(&config, database)?;
        Ok(Task { services, shared })
    }

    #[cfg(test)]
    pub fn sub_services(&mut self) -> &mut SubServices {
        &mut self.services
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
        self,
        watcher: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
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

        // Concurrently start all sub-services to reduce startup time.
        futures::future::try_join_all(
            self.services.iter().map(|service| service.start_and_await())
        ).await?;
       
        Ok(self)
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    #[tracing::instrument(skip_all)]
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let stop_signals = self.services.iter()
            .map(|service| service.await_stop())
            .chain(std::iter::once(Box::pin(watcher.while_started())));

        if let Err(err) = futures::future::select_all(stop_signals).await.0 {
            tracing::error!("Error during shutdown listening: {}", err);
        }

        // Received stop signal, stop this service and all sub-services.
        Ok(false)
    }

    
    async fn shutdown(self) -> anyhow::Result<()> {
        let stop_futures = self.services.into_iter().map(|service| {
            service.stop_and_await().map(|result| {
                if let Err(err) = result {
                    tracing::error!("Error during service stop: {}", err);
                }
            })
        });

        // Concurrently wait for all services to stop.
        futures::future::join_all(stop_futures).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::service::{
        Config,
        Task,
    };
    use fuel_core_services::{
        RunnableService,
        RunnableTask,
        State,
    };
    use std::{
        thread::sleep,
        time::Duration,
    };

    #[tokio::test]
    async fn run_start_and_stop() {
        // The test verify that if we stop any of sub-services
        let mut i = 0;
        loop {
            let task = Task::new(Default::default(), Config::local_node()).unwrap();
            let (_, receiver) = tokio::sync::watch::channel(State::NotStarted);
            let mut watcher = receiver.into();
            let mut task = task.into_task(&watcher, ()).await.unwrap();
            sleep(Duration::from_secs(1));
            for service in task.sub_services() {
                assert_eq!(service.state(), State::Started);
            }

            if i < task.sub_services().len() {
                task.sub_services()[i].stop_and_await().await.unwrap();
                assert!(!task.run(&mut watcher).await.unwrap());
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
    async fn shutdown_stops_all_services() {
        let task = Task::new(Default::default(), Config::local_node()).unwrap();
        let mut task = task.into_task(&Default::default(), ()).await.unwrap();
        let sub_services_watchers: Vec<_> = task
            .sub_services()
            .iter()
            .map(|s| s.state_watcher())
            .collect();

        sleep(Duration::from_secs(1));
        for service in task.sub_services() {
            assert_eq!(service.state(), State::Started);
        }
        task.shutdown().await.unwrap();

        for mut service in sub_services_watchers {
            // Check that the state is `Stopped`(not `StoppedWithError`)
            assert_eq!(service.borrow_and_update().clone(), State::Stopped);
        }
    }
}
