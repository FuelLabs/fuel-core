use crate::{
    database::Database,
    service::adapters::P2PAdapter,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    State,
    StateWatcher,
};
use std::net::SocketAddr;
use tracing::log::warn;

pub use config::{
    Config,
    DbType,
    VMConfig,
};
pub use fuel_core_services::Service as ServiceTrait;

use self::adapters::BlockImporterAdapter;

pub mod adapters;
pub mod config;
pub mod genesis;
pub mod metrics;
pub mod sub_services;

#[derive(Clone)]
pub struct SharedState {
    /// The transaction pool shared state.
    pub txpool: fuel_core_txpool::service::SharedState<P2PAdapter, Database>,
    /// The P2P network shared state.
    #[cfg(feature = "p2p")]
    pub network: fuel_core_p2p::service::SharedState,
    #[cfg(feature = "relayer")]
    /// The Relayer shared state.
    pub relayer: Option<fuel_core_relayer::SharedState>,
    /// The GraphQL shared state.
    pub graph_ql: crate::fuel_core_graphql_api::service::SharedState,
    /// Subscribe to new block production.
    pub block_importer: BlockImporterAdapter,
    #[cfg(feature = "test-helpers")]
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

impl FuelService {
    /// Creates a `FuelService` instance from service config
    #[tracing::instrument(skip_all, fields(name = %config.name))]
    pub fn new(database: Database, mut config: Config) -> anyhow::Result<Self> {
        Self::make_config_consistent(&mut config);
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
        let database = match config.database_type {
            #[cfg(feature = "rocksdb")]
            DbType::RocksDb => Database::open(&config.database_path)?,
            DbType::InMemory => Database::in_memory(),
            #[cfg(not(feature = "rocksdb"))]
            _ => Database::in_memory(),
        };

        Self::from_database(database, config).await
    }

    /// Creates and starts fuel node instance from service config and a pre-existing database
    pub async fn from_database(
        database: Database,
        config: Config,
    ) -> anyhow::Result<Self> {
        let service = Self::new(database, config)?;
        service.runner.start_and_await().await?;
        Ok(service)
    }

    #[cfg(feature = "relayer")]
    /// Wait for the [`Relayer`] to be in sync with
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

    // TODO: Rework our configs system to avoid nesting of the same configs.
    fn make_config_consistent(config: &mut Config) {
        if config.txpool.chain_config != config.chain_conf {
            warn!("The `ChainConfig` of `TxPool` was inconsistent");
            config.txpool.chain_config = config.chain_conf.clone();
        }
        if config.txpool.utxo_validation != config.utxo_validation {
            warn!("The `utxo_validation` of `TxPool` was inconsistent");
            config.txpool.utxo_validation = config.utxo_validation;
        }
        if config.block_producer.utxo_validation != config.utxo_validation {
            warn!("The `utxo_validation` of `BlockProducer` was inconsistent");
            config.block_producer.utxo_validation = config.utxo_validation;
        }
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
    pub fn new(database: Database, config: Config) -> anyhow::Result<Task> {
        // initialize state
        genesis::maybe_initialize_state(&config, &database)?;

        // initialize sub services
        let (services, shared) = sub_services::init_sub_services(&config, &database)?;
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

    fn shared_data(&self) -> Self::SharedData {
        self.shared.clone()
    }

    async fn into_task(self, _: &StateWatcher) -> anyhow::Result<Self::Task> {
        for service in &self.services {
            service.start_and_await().await?;
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    #[tracing::instrument(skip_all)]
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let mut stop_signals = vec![];
        for service in &self.services {
            stop_signals.push(service.await_stop())
        }
        stop_signals.push(Box::pin(watcher.while_started()));

        let (result, _, _) = futures::future::select_all(stop_signals).await;

        if let Err(err) = result {
            tracing::error!("Got an error during listen for shutdown: {}", err);
        }

        // We received the stop signal from any of one source, so stop this service and
        // all sub-services.
        for service in &self.services {
            let result = service.stop_and_await().await;

            if let Err(err) = result {
                tracing::error!(
                    "Got and error during awaiting for stop of the service: {}",
                    err
                );
            }
        }

        Ok(false /* should_continue */)
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
            let mut task = task.into_task(&watcher).await.unwrap();
            sleep(Duration::from_secs(1));
            for service in task.sub_services() {
                assert_eq!(service.state(), State::Started);
            }

            if i < task.sub_services().len() {
                task.sub_services()[i].stop_and_await().await.unwrap();
                assert!(!task.run(&mut watcher).await.unwrap());

                for service in task.sub_services() {
                    // Check that the state is `Stopped`(not `StoppedWithError`)
                    assert_eq!(service.state(), State::Stopped);
                }
            } else {
                break
            }
            i += 1;
        }

        #[allow(unused_mut)]
        let mut expected_services = 3;

        // Relayer service is disabled with `Config::local_node`.
        // #[cfg(feature = "relayer")]
        // {
        //     expected_services += 1;
        // }
        #[cfg(feature = "p2p")]
        {
            // p2p
            expected_services += 1;
        }

        // # Dev-note: Update the `expected_services` when we add/remove a new/old service.
        assert_eq!(i, expected_services);
    }
}
