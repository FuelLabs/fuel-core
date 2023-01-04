use crate::database::Database;
use anyhow::Error as AnyError;
use futures::FutureExt;
use std::{
    net::SocketAddr,
    panic,
};
use tokio::{
    sync::oneshot,
    task::JoinHandle,
};
use tracing::log::warn;

pub use config::{
    Config,
    DbType,
    VMConfig,
};

pub mod adapters;
pub mod config;
pub(crate) mod genesis;
pub mod graph_api;
pub mod metrics;
pub mod modules;

pub struct SharedState {
    #[cfg(feature = "relayer")]
    /// The Relayer shared state.
    pub relayer: Option<fuel_core_relayer::SharedState>,
    /// The GraphQL shared state.
    pub graph_ql: crate::graph_api::service::SharedState,
}

pub struct FuelService {
    handle: JoinHandle<()>,
    /// Shutdown the fuel service.
    shutdown: oneshot::Sender<()>,
    #[cfg(feature = "relayer")]
    /// Relayer handle
    relayer_handle: Option<fuel_core_relayer::SharedState>,
    /// The address bound by the system for serving the API
    pub bound_address: SocketAddr,
}

struct Task {
    tasks: Vec<JoinHandle<Result<(), AnyError>>>,
    /// The address bound by the system for serving the API
    pub shared: SharedState,
}

impl FuelService {
    /// Create a fuel node instance from service config
    #[tracing::instrument(skip(config))]
    pub async fn new_node(mut config: Config) -> Result<Self, AnyError> {
        Self::make_config_consistent(&mut config);
        // initialize database
        let database = match config.database_type {
            #[cfg(feature = "rocksdb")]
            DbType::RocksDb => Database::open(&config.database_path)?,
            DbType::InMemory => Database::in_memory(),
            #[cfg(not(feature = "rocksdb"))]
            _ => Database::in_memory(),
        };
        // initialize service
        Ok(Self::spawn_service(
            Self::init_service(database, config).await?,
        ))
    }

    fn spawn_service(service: Task) -> Self {
        let bound_address = service.shared.graph_ql.bound_addr;
        let (shutdown, stop_rx) = oneshot::channel();

        #[cfg(feature = "relayer")]
        let relayer_handle = service.shared.relayer.clone();

        let handle = tokio::spawn(async move {
            let run_fut = service.run();
            let shutdown_fut = stop_rx.then(|stop| async move {
                if stop.is_err() {
                    // If the handle is dropped we don't want
                    // this to ever shutdown the service.
                    futures::future::pending::<()>().await;
                }
                // Only a successful recv results in a shutdown.
            });
            tokio::pin!(run_fut);
            tokio::pin!(shutdown_fut);
            futures::future::select(shutdown_fut, run_fut).await;
        });
        Self {
            handle,
            shutdown,
            bound_address,
            #[cfg(feature = "relayer")]
            relayer_handle,
        }
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

    /// Used to initialize a service with a pre-existing database
    pub async fn from_database(
        database: Database,
        config: Config,
    ) -> Result<Self, AnyError> {
        Ok(Self::spawn_service(
            Self::init_service(database, config).await?,
        ))
    }

    /// Private inner method for initializing the fuel service
    async fn init_service(database: Database, config: Config) -> Result<Task, AnyError> {
        // initialize state
        Self::initialize_state(&config, &database)?;

        // start modules
        let modules = modules::start_modules(&config, &database).await?;
        let shared = SharedState {
            #[cfg(feature = "relayer")]
            relayer: modules.relayer.map(|r| r.shared.clone()),
            graph_ql: modules.graph_ql.shared,
        };

        // start background tasks
        let tasks = vec![];
        Ok(Task { tasks, shared })
    }

    /// Awaits for the completion of any server background tasks
    pub async fn run(self) {
        Self::wait_for_handle(self.handle).await;
    }

    /// Shutdown background tasks
    pub async fn stop(self) {
        let Self {
            handle, shutdown, ..
        } = self;
        let _ = shutdown.send(());
        Self::wait_for_handle(handle).await;
    }

    async fn wait_for_handle(handle: JoinHandle<()>) {
        if let Err(err) = handle.await {
            if err.is_panic() {
                // Resume the panic on the main task
                panic::resume_unwind(err.into_panic());
            }
        }
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
        if let Some(relayer_handle) = &self.relayer_handle {
            relayer_handle.await_synced().await?;
        }
        Ok(())
    }
}

impl Task {
    /// Awaits for the completion of any server background tasks
    pub async fn run(self) {
        let Self { tasks, .. } = self;
        let run_fut = Self::run_inner(tasks);
        let shutdown_fut = shutdown_signal();
        tokio::pin!(run_fut);
        tokio::pin!(shutdown_fut);
        futures::future::select(shutdown_fut, run_fut).await;
        // TODO: Stop after receiving shutdown signal
        // modules.stop().await;
    }

    /// Awaits for the completion of any server background tasks
    async fn run_inner(tasks: Vec<JoinHandle<anyhow::Result<()>>>) {
        for task in tasks {
            match task.await {
                Err(err) => {
                    if err.is_panic() {
                        // Resume the panic on the main task
                        panic::resume_unwind(err.into_panic());
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("server error: {:?}", e);
                }
                Ok(Ok(_)) => {}
            }
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install sigterm handler");

        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .expect("failed to install sigint handler");
        loop {
            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("sigterm received");
                    break;
                }
                _ = sigint.recv() => {
                    tracing::log::info!("sigint received");
                    break;
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
        tracing::log::info!("CTRL+C received");
    }
}
