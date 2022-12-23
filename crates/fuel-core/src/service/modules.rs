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
use fuel_core_txpool::{
    ports::TxPoolDb,
    service::TxStatusChange,
};
use futures::future::join_all;
use std::sync::Arc;
use tokio::{
    sync::{
        broadcast,
        mpsc,
        Mutex,
        Semaphore,
    },
    task::JoinHandle,
};

type PoAService = fuel_core_poa::Service<Database, TxPoolAdapter, BlockProducerAdapter>;
#[cfg(feature = "relayer")]
type RelayerService = fuel_core_relayer::Service;
type TxPoolService = fuel_core_txpool::Service;

pub struct Modules {
    pub txpool: TxPoolService,
    pub block_importer: Arc<fuel_core_importer::Service>,
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
    pub consensus_module: PoAService,
    pub sync: Arc<fuel_core_sync::Service>,
    #[cfg(feature = "relayer")]
    pub relayer: Option<RelayerService>,
    #[cfg(feature = "p2p")]
    pub network_service: P2PAdapter,
}

impl Modules {
    pub async fn stop(&self) {
        self.consensus_module.stop_and_await().await.unwrap();
        self.txpool.stop_and_await().await.unwrap();
        let stops: Vec<JoinHandle<()>> = vec![
            #[cfg(feature = "p2p")]
            self.network_service.stop().await,
            self.block_importer.stop().await,
            self.sync.stop().await,
        ]
        .into_iter()
        .flatten()
        .collect();

        join_all(stops).await;
    }
}

pub async fn start_modules(
    config: &Config,
    database: &Database,
) -> anyhow::Result<Modules> {
    let db = ();

    // Initialize and bind all components
    let block_importer =
        fuel_core_importer::Service::new(&config.block_importer, db).await?;
    let sync = fuel_core_sync::Service::new(&config.sync).await?;

    #[cfg(feature = "relayer")]
    let relayer = if config.relayer.eth_client.is_some() {
        Some(fuel_core_relayer::new_service(
            Box::new(database.clone()),
            config.relayer.clone(),
        )?)
    } else {
        None
    };

    let (_block_event_sender, block_event_receiver) = mpsc::channel(100);
    let (block_import_tx, _) = broadcast::channel(16);

    #[cfg(feature = "p2p")]
    let network_service = {
        let p2p_db = Arc::new(database.clone());

        let genesis = database.get_genesis()?;
        let p2p_config = config.p2p.clone().init(genesis)?;

        Arc::new(fuel_core_p2p::orchestrator::Service::new(
            p2p_config, p2p_db,
        ))
    };

    #[cfg(feature = "p2p")]
    let p2p_adapter = P2PAdapter::new(network_service);
    #[cfg(not(feature = "p2p"))]
    let p2p_adapter = P2PAdapter::new();

    let p2p_adapter = p2p_adapter;
    p2p_adapter.start().await?;

    let tx_status_sender = TxStatusChange::new(100);

    let mut txpool_builder = fuel_core_txpool::ServiceBuilder::new();
    txpool_builder
        .config(config.txpool.clone())
        .db(Arc::new(database.clone()) as Arc<dyn TxPoolDb>)
        .p2p(Box::new(p2p_adapter.clone()))
        .importer(Box::new(BlockImportAdapter::new(block_import_tx.clone())))
        .tx_status_sender(tx_status_sender.clone());

    let txpool_service = txpool_builder.build()?;

    // restrict the max number of concurrent dry runs to the number of CPUs
    // as execution in the worst case will be CPU bound rather than I/O bound.
    let max_dry_run_concurrency = num_cpus::get();
    let block_producer = Arc::new(fuel_core_producer::Producer {
        config: config.block_producer.clone(),
        db: database.clone(),
        txpool: Box::new(TxPoolAdapter::new(txpool_service.clone())),
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

    block_importer.start().await;

    let poa = match &config.chain_conf.block_production {
        BlockProduction::ProofOfAuthority { trigger } => fuel_core_poa::new_service(
            fuel_core_poa::Config {
                trigger: *trigger,
                block_gas_limit: config.chain_conf.block_gas_limit,
                signing_key: config.consensus_key.clone(),
                metrics: false,
            },
            TxPoolAdapter::new(txpool_service.clone()),
            block_import_tx,
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

    sync.start(
        block_event_receiver,
        // TODO: re-introduce this when sync actually depends on the coordinator
        // bft.sender().clone(),
        block_importer.sender().clone(),
    )
    .await;

    Ok(Modules {
        txpool: txpool_service,
        block_importer: Arc::new(block_importer),
        block_producer,
        consensus_module: poa,
        sync: Arc::new(sync),
        #[cfg(feature = "relayer")]
        relayer,
        #[cfg(feature = "p2p")]
        network_service: p2p_adapter,
    })
}
