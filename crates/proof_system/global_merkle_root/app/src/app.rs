use std::num::NonZeroU64;

use fuel_core::state::{
    historical_rocksdb::StateRewindPolicy,
    rocks_db::{
        ColumnsPolicy,
        DatabaseConfig,
    },
};
use fuel_core_global_merkle_root_api::service::StateRootApiService;
use fuel_core_global_merkle_root_service::service::UpdateMerkleRootTask;
use fuel_core_services::{
    Service,
    ServiceRunner,
};
use fuel_core_types::fuel_types::ChainId;
use tracing_subscriber::EnvFilter;

use crate::{
    adapters::{
        blocks::BlockStreamAdapter,
        db::StateRootDb,
    },
    cli::{
        self,
        DbType,
    },
};

/// App struct, holds handles to the services in the app.
pub struct App {
    merkle_root_service:
        ServiceRunner<UpdateMerkleRootTask<BlockStreamAdapter, StateRootDb>>,
    api_service: ServiceRunner<StateRootApiService>,
}

impl App {
    /// Create a new app.
    pub async fn new(args: &cli::Args) -> anyhow::Result<Self> {
        let db = open_db(args)?;
        let height = u32::from(db.block_height()?);
        let block_stream =
            BlockStreamAdapter::new(&args.fuel_node_url, height, args.batch_size)?;
        let chain_id = ChainId::new(args.chain_id);

        let merkle_root_service =
            fuel_core_global_merkle_root_service::service::new_service(
                chain_id,
                db.clone(),
                block_stream,
            );

        // Services
        let (host, port) = (&args.host, args.port);
        let network_address = format!("{host}:{port}");
        let api_service =
            fuel_core_global_merkle_root_api::service::new_service(db, network_address)?;

        Ok(Self {
            merkle_root_service,
            api_service,
        })
    }

    /// Start the app and run until receiving a ctrl-c signal.
    pub async fn run(&self, args: &cli::Args) -> anyhow::Result<()> {
        setup_logging(args.log_format)?;

        self.start().await?;

        tokio::signal::ctrl_c().await?;
        tracing::info!("Received Ctrl-C - shutting down");
        self.stop().await?;

        Ok(())
    }

    /// Start the app.
    pub async fn start(&self) -> anyhow::Result<()> {
        self.merkle_root_service.start_and_await().await?;
        self.api_service.start_and_await().await?;

        Ok(())
    }

    /// Stop the app.
    pub async fn stop(&self) -> anyhow::Result<()> {
        self.merkle_root_service.stop_and_await().await?;
        self.api_service.stop_and_await().await?;

        Ok(())
    }
}

fn setup_logging(format: cli::LogFormat) -> anyhow::Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let builder = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .with_line_number(true)
        .with_level(true);

    match format {
        cli::LogFormat::JSON => tracing::subscriber::set_global_default(
            builder.with_ansi(false).json().finish(),
        )?,
        cli::LogFormat::Human => {
            tracing::subscriber::set_global_default(builder.with_ansi(true).finish())?
        }
    };

    Ok(())
}

fn open_db(args: &cli::Args) -> anyhow::Result<StateRootDb> {
    let state_rewind_policy = StateRewindPolicy::RewindRange {
        size: NonZeroU64::new(14).unwrap(),
    };

    let database_config = DatabaseConfig {
        cache_capacity: args.db_cache_capacity,
        max_fds: args.db_max_files,
        columns_policy: ColumnsPolicy::OnCreation,
    };

    let db = match args.db_type {
        DbType::InMemory => StateRootDb::in_memory(),
        DbType::RocksDb => StateRootDb::open_rocksdb(
            &args.db_path,
            state_rewind_policy,
            database_config,
        )?,
    };

    Ok(db)
}
