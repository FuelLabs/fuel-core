use std::num::NonZeroU64;

use fuel_core::state::{
    historical_rocksdb::StateRewindPolicy,
    rocks_db::{
        ColumnsPolicy,
        DatabaseConfig,
    },
};
use fuel_core_services::Service;
use fuel_core_types::fuel_types::ChainId;

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

pub async fn run(args: &cli::Args) -> anyhow::Result<()> {
    // Adapters
    let db = open_db(args)?;
    let height = u32::from(db.block_height()?);
    let block_stream =
        BlockStreamAdapter::new(&args.fuel_node_url, height, args.batch_size)?;
    let chain_id = ChainId::new(args.chain_id);

    let merkle_root_service = fuel_core_global_merkle_root_service::service::new_service(
        chain_id,
        db.clone(),
        block_stream,
    );

    // Services
    let (host, port) = (&args.host, args.port);
    let network_address = format!("{host}:{port}");
    let api_service =
        fuel_core_global_merkle_root_api::service::new_service(db, network_address)?;

    merkle_root_service.start_and_await().await?;
    api_service.start_and_await().await?;

    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    // TODO: Graceful shutdown
    Ok(())
}

fn open_db(args: &cli::Args) -> anyhow::Result<StateRootDb> {
    let state_rewind_policy = StateRewindPolicy::RewindRange {
        size: NonZeroU64::new(14).unwrap(),
    };

    // TODO: Expose parameters?
    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 1024,
        columns_policy: ColumnsPolicy::OnCreation,
    };

    let db = match args.database_type {
        DbType::InMemory => StateRootDb::in_memory(),
        DbType::RocksDb => StateRootDb::open_rocksdb(
            &args.database_path,
            state_rewind_policy,
            database_config,
        )?,
    };

    Ok(db)
}
