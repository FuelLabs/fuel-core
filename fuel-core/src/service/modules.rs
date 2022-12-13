#![allow(clippy::let_unit_value)]
use crate::{
    chain_config::BlockProduction,
    database::Database,
    service::{
        adapters::{
            ExecutorAdapter,
            MaybeRelayerAdapter,
            PoACoordinatorAdapter,
        },
        Config,
    },
};
use anyhow::Result;
use fuel_core_interfaces::{
    self,
    txpool::{
        Sender,
        TxPoolDb,
    },
};
use fuel_txpool::service::TxStatusChange;
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

#[cfg(feature = "p2p")]
use fuel_core_interfaces::p2p::P2pDb;

pub struct Modules {
    pub txpool: Arc<fuel_txpool::Service>,
    pub block_importer: Arc<fuel_block_importer::Service>,
    pub block_producer: Arc<fuel_block_producer::Producer<Database>>,
    pub coordinator: Arc<CoordinatorService>,
    pub sync: Arc<fuel_sync::Service>,
    #[cfg(feature = "relayer")]
    pub relayer: Option<fuel_relayer::RelayerHandle>,
    #[cfg(feature = "p2p")]
    pub network_service: Arc<fuel_p2p::orchestrator::Service>,
}

impl Modules {
    pub async fn stop(&self) {
        let stops: Vec<JoinHandle<()>> = vec![
            self.coordinator.stop().await,
            #[cfg(feature = "p2p")]
            self.network_service.stop().await,
            self.txpool.stop().await,
            self.block_importer.stop().await,
            self.sync.stop().await,
        ]
        .into_iter()
        .flatten()
        .collect();

        join_all(stops).await;
    }
}

pub enum CoordinatorService {
    Poa(fuel_poa_coordinator::Service),
    Bft(fuel_core_bft::Service),
}
impl CoordinatorService {
    pub async fn stop(&self) -> Option<tokio::task::JoinHandle<()>> {
        match self {
            CoordinatorService::Poa(s) => s.stop().await,
            CoordinatorService::Bft(s) => s.stop().await,
        }
    }
}

pub async fn start_modules(config: &Config, database: &Database) -> Result<Modules> {
    let db = ();

    // Initialize and bind all components
    let block_importer =
        fuel_block_importer::Service::new(&config.block_importer, db).await?;
    let sync = fuel_sync::Service::new(&config.sync).await?;

    let coordinator = match &config.chain_conf.block_production {
        BlockProduction::ProofOfAuthority { trigger } => CoordinatorService::Poa(
            fuel_poa_coordinator::Service::new(&fuel_poa_coordinator::Config {
                trigger: *trigger,
                block_gas_limit: config.chain_conf.block_gas_limit,
                signing_key: config.consensus_key.clone(),
                metrics: false,
            }),
        ),
        // TODO: enable when bft config is ready to use
        // CoordinatorConfig::Bft { config } => {
        //     CoordinatorService::Bft(fuel_core_bft::Service::new(config, db).await?)
        // }
    };

    #[cfg(feature = "relayer")]
    let relayer = if config.relayer.eth_client.is_some() {
        Some(fuel_relayer::RelayerHandle::start(
            Box::new(database.clone()),
            config.relayer.clone(),
        )?)
    } else {
        None
    };

    let (incoming_tx_sender, incoming_tx_receiver) = broadcast::channel(100);
    let (block_event_sender, block_event_receiver) = mpsc::channel(100);
    let (block_import_tx, block_import_rx) = broadcast::channel(16);

    #[cfg(feature = "p2p")]
    let (p2p_request_event_sender, p2p_request_event_receiver) = mpsc::channel(100);
    #[cfg(not(feature = "p2p"))]
    let (p2p_request_event_sender, mut p2p_request_event_receiver) = mpsc::channel(100);

    #[cfg(feature = "p2p")]
    let network_service = {
        let p2p_db: Arc<dyn P2pDb> = Arc::new(database.clone());
        let (tx_consensus, _) = mpsc::channel(100);

        let genesis = database.get_genesis()?;
        let p2p_config = config.p2p.clone().init(genesis)?;

        fuel_p2p::orchestrator::Service::new(
            p2p_config,
            p2p_db,
            p2p_request_event_receiver,
            tx_consensus,
            incoming_tx_sender,
            block_event_sender,
        )
    };

    #[cfg(not(feature = "p2p"))]
    {
        let keep_alive = Box::new(incoming_tx_sender);
        Box::leak(keep_alive);

        let keep_alive = Box::new(block_event_sender);
        Box::leak(keep_alive);

        tokio::spawn(async move {
            while (p2p_request_event_receiver.recv().await).is_some() {}
        });
    }

    let tx_status_sender = TxStatusChange::new(100);

    let (txpool_sender, txpool_receiver) = mpsc::channel(100);

    let mut txpool_builder = fuel_txpool::ServiceBuilder::new();
    txpool_builder
        .config(config.txpool.clone())
        .db(Box::new(database.clone()) as Box<dyn TxPoolDb>)
        .incoming_tx_receiver(incoming_tx_receiver)
        .import_block_event(block_import_rx)
        .tx_status_sender(tx_status_sender.clone())
        .txpool_sender(Sender::new(txpool_sender))
        .txpool_receiver(txpool_receiver);

    txpool_builder.network_sender(p2p_request_event_sender.clone());

    // restrict the max number of concurrent dry runs to the number of CPUs
    // as execution in the worst case will be CPU bound rather than I/O bound.
    let max_dry_run_concurrency = num_cpus::get();
    let block_producer = Arc::new(fuel_block_producer::Producer {
        config: config.block_producer.clone(),
        db: database.clone(),
        txpool: Box::new(fuel_block_producer::adapters::TxPoolAdapter {
            sender: txpool_builder.sender().clone(),
        }),
        executor: Arc::new(ExecutorAdapter {
            database: database.clone(),
            config: config.clone(),
        }),
        relayer: Box::new(MaybeRelayerAdapter {
            database: database.clone(),
            #[cfg(feature = "relayer")]
            relayer_synced: relayer.as_ref().map(|r| r.listen_synced()),
        }),
        lock: Mutex::new(()),
        dry_run_semaphore: Semaphore::new(max_dry_run_concurrency),
    });

    // start services

    block_importer.start().await;

    match &coordinator {
        CoordinatorService::Poa(poa) => {
            poa.start(
                txpool_builder.tx_status_subscribe(),
                txpool_builder.sender().clone(),
                block_import_tx,
                PoACoordinatorAdapter {
                    block_producer: block_producer.clone(),
                },
                database.clone(),
            )
            .await;
        }
        CoordinatorService::Bft(bft) => {
            bft.start(
                p2p_request_event_sender.clone(),
                block_importer.sender().clone(),
                block_importer.subscribe(),
            )
            .await;
        }
    }

    sync.start(
        block_event_receiver,
        p2p_request_event_sender.clone(),
        // TODO: re-introduce this when sync actually depends on the coordinator
        // bft.sender().clone(),
        block_importer.sender().clone(),
    )
    .await;

    #[cfg(feature = "p2p")]
    if !config.p2p.network_name.is_empty() {
        network_service.start().await?;
    }

    let txpool = txpool_builder.build()?;
    txpool.start().await?;

    Ok(Modules {
        txpool: Arc::new(txpool),
        block_importer: Arc::new(block_importer),
        block_producer,
        coordinator: Arc::new(coordinator),
        sync: Arc::new(sync),
        #[cfg(feature = "relayer")]
        relayer,
        #[cfg(feature = "p2p")]
        network_service: Arc::new(network_service),
    })
}
