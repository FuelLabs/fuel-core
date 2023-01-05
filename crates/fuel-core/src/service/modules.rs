#![allow(clippy::let_unit_value)]
use super::adapters::P2PAdapter;
use crate::{
    chain_config::BlockProduction,
    database::Database,
    service::{
        adapters::{
            BlockImportAdapter,
            BlockProducerAdapter,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            TxPoolAdapter,
        },
        Config,
    },
};
use fuel_core_services::Service as ServiceTrait;
use fuel_core_txpool::service::TxStatusChange;
use std::sync::Arc;
use tokio::sync::{
    broadcast,
    Mutex,
    Semaphore,
};

pub type PoAService =
    fuel_core_poa::Service<Database, TxPoolAdapter, BlockProducerAdapter>;
#[cfg(feature = "relayer")]
pub type RelayerService = fuel_core_relayer::Service<Database>;
#[cfg(feature = "p2p")]
pub type P2PService = fuel_core_p2p::service::Service<Database>;
pub type TxPoolService = fuel_core_txpool::Service<P2PAdapter, Database>;

pub struct Modules {
    pub txpool: TxPoolService,
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
    pub consensus_module: PoAService,
    #[cfg(feature = "relayer")]
    pub relayer: Option<RelayerService>,
    #[cfg(feature = "p2p")]
    pub network_service: P2PService,
}

impl Modules {
    pub async fn stop(&self) {
        self.consensus_module.stop_and_await().await.unwrap();
        self.txpool.stop_and_await().await.unwrap();
        #[cfg(feature = "p2p")]
        self.network_service.stop_and_await().await.unwrap();
    }
}

pub async fn start_modules(
    config: &Config,
    database: &Database,
) -> anyhow::Result<Modules> {
    #[cfg(feature = "relayer")]
    let relayer = if config.relayer.eth_client.is_some() {
        Some(fuel_core_relayer::new_service(
            database.clone(),
            config.relayer.clone(),
        )?)
    } else {
        None
    };

    let (block_import_tx, _) = broadcast::channel(16);

    #[cfg(feature = "p2p")]
    let network_service = {
        let p2p_db = database.clone();

        let genesis = p2p_db.get_genesis()?;
        let p2p_config = config.p2p.clone().init(genesis)?;

        fuel_core_p2p::service::new_service(p2p_config, p2p_db)
    };

    #[cfg(feature = "p2p")]
    let p2p_adapter = P2PAdapter::new(network_service.shared.clone());
    #[cfg(not(feature = "p2p"))]
    let p2p_adapter = P2PAdapter::new();

    let p2p_adapter = p2p_adapter;

    let importer_adapter = BlockImportAdapter::new(block_import_tx);

    let txpool_service = fuel_core_txpool::new_service(
        config.txpool.clone(),
        database.clone(),
        TxStatusChange::new(100),
        importer_adapter.clone(),
        p2p_adapter,
    );

    // restrict the max number of concurrent dry runs to the number of CPUs
    // as execution in the worst case will be CPU bound rather than I/O bound.
    let max_dry_run_concurrency = num_cpus::get();
    let block_producer = Arc::new(fuel_core_producer::Producer {
        config: config.block_producer.clone(),
        db: database.clone(),
        txpool: Box::new(TxPoolAdapter::new(txpool_service.shared.clone())),
        executor: Arc::new(ExecutorAdapter {
            database: database.clone(),
            config: config.clone(),
        }),
        relayer: Box::new(MaybeRelayerAdapter {
            database: database.clone(),
            #[cfg(feature = "relayer")]
            relayer_synced: relayer.as_ref().map(|r| r.shared.clone()),
        }),
        lock: Mutex::new(()),
        dry_run_semaphore: Semaphore::new(max_dry_run_concurrency),
    });

    // start services

    let poa = match &config.chain_conf.block_production {
        BlockProduction::ProofOfAuthority { trigger } => fuel_core_poa::new_service(
            fuel_core_poa::Config {
                trigger: *trigger,
                block_gas_limit: config.chain_conf.block_gas_limit,
                signing_key: config.consensus_key.clone(),
                metrics: false,
            },
            TxPoolAdapter::new(txpool_service.shared.clone()),
            // TODO: Pass Importer
            importer_adapter.tx,
            BlockProducerAdapter {
                block_producer: block_producer.clone(),
            },
            database.clone(),
        ),
    };

    poa.start()?;
    #[cfg(feature = "relayer")]
    if let Some(relayer) = relayer.as_ref() {
        relayer.start().expect("Should start relayer")
    }
    txpool_service.start()?;
    #[cfg(feature = "p2p")]
    network_service.start()?;

    Ok(Modules {
        txpool: txpool_service,
        block_producer,
        consensus_module: poa,
        #[cfg(feature = "relayer")]
        relayer,
        #[cfg(feature = "p2p")]
        network_service,
    })
}
