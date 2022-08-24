#![allow(clippy::let_unit_value)]
use crate::database::Database;
use crate::service::Config;
use anyhow::Result;
#[cfg(feature = "p2p")]
use fuel_core_interfaces::p2p::P2pDb;
#[cfg(feature = "relayer")]
use fuel_core_interfaces::relayer::RelayerDb;
use fuel_core_interfaces::txpool::TxPoolDb;
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct Modules {
    pub txpool: Arc<fuel_txpool::Service>,
    pub block_importer: Arc<fuel_block_importer::Service>,
    pub block_producer: Arc<fuel_block_producer::Service>,
    pub bft: Arc<fuel_core_bft::Service>,
    pub sync: Arc<fuel_sync::Service>,
    #[cfg(feature = "relayer")]
    pub relayer: Arc<fuel_relayer::Service>,
    #[cfg(feature = "p2p")]
    pub network_service: Arc<fuel_p2p::orchestrator::Service>,
}

impl Modules {
    pub async fn stop(&self) {
        let stops: Vec<JoinHandle<()>> = vec![
            self.txpool.stop().await,
            self.block_importer.stop().await,
            self.block_producer.stop().await,
            self.bft.stop().await,
            self.sync.stop().await,
            #[cfg(feature = "p2p")]
            self.network_service.stop().await,
        ]
        .into_iter()
        .flatten()
        .collect();

        join_all(stops).await;
    }
}

pub async fn start_modules(config: &Config, database: &Database) -> Result<Modules> {
    let db = ();
    // Initialize and bind all components
    let block_importer = fuel_block_importer::Service::new(&config.block_importer, db).await?;
    let block_producer = fuel_block_producer::Service::new(&config.block_producer, db).await?;
    let bft = fuel_core_bft::Service::new(&config.bft, db).await?;
    let sync = fuel_sync::Service::new(&config.sync).await?;

    // create builders
    #[cfg(feature = "relayer")]
    let mut relayer_builder = fuel_relayer::ServiceBuilder::new();
    let mut txpool_builder = fuel_txpool::ServiceBuilder::new();

    // initiate fields for builders
    #[cfg(feature = "relayer")]
    relayer_builder
        .config(config.relayer.clone())
        .db(Box::new(database.clone()) as Box<dyn RelayerDb>)
        .import_block_event(block_importer.subscribe())
        .private_key(
            hex::decode("c6bd905dcac2a0b1c43f574ab6933df14d7ceee0194902bce523ed054e8e798b")
                .unwrap(),
        );

    let relayer_sender = {
        #[cfg(feature = "relayer")]
        {
            relayer_builder.sender().clone()
        }
        #[cfg(not(feature = "relayer"))]
        {
            fuel_core_interfaces::relayer::Sender::noop()
        }
    };

    txpool_builder
        .config(config.txpool.clone())
        .db(Box::new(database.clone()) as Box<dyn TxPoolDb>)
        .import_block_event(block_importer.subscribe());

    #[cfg(feature = "p2p")]
    let (tx_request_event, rx_request_event) = mpsc::channel(100);
    #[cfg(feature = "p2p")]
    let (tx_block, rx_block) = mpsc::channel(100);

    #[cfg(not(feature = "p2p"))]
    let (tx_request_event, _) = mpsc::channel(100);
    #[cfg(not(feature = "p2p"))]
    let (_, rx_block) = mpsc::channel(100);

    block_importer.start().await;

    block_producer.start(txpool_builder.sender().clone()).await;
    bft.start(
        relayer_sender.clone(),
        tx_request_event.clone(),
        block_producer.sender().clone(),
        block_importer.sender().clone(),
        block_importer.subscribe(),
    )
    .await;

    sync.start(
        rx_block,
        tx_request_event.clone(),
        relayer_sender,
        bft.sender().clone(),
        block_importer.sender().clone(),
    )
    .await;

    // build services
    #[cfg(feature = "relayer")]
    let relayer = relayer_builder.build()?;
    let txpool = txpool_builder.build()?;

    // start services
    #[cfg(feature = "relayer")]
    if config.relayer.eth_client.is_some() {
        relayer.start().await?;
    }
    txpool.start().await?;

    #[cfg(feature = "p2p")]
    let p2p_db: Arc<dyn P2pDb> = Arc::new(database.clone());
    #[cfg(feature = "p2p")]
    let (tx_consensus, _) = mpsc::channel(100);
    #[cfg(feature = "p2p")]
    let (tx_transaction, _) = mpsc::channel(100);

    #[cfg(feature = "p2p")]
    let network_service = fuel_p2p::orchestrator::Service::new(
        config.p2p.clone(),
        p2p_db,
        tx_request_event,
        rx_request_event,
        tx_consensus,
        tx_transaction,
        tx_block,
    );

    #[cfg(feature = "p2p")]
    if !config.p2p.network_name.is_empty() {
        network_service.start().await?;
    }

    Ok(Modules {
        txpool: Arc::new(txpool),
        block_importer: Arc::new(block_importer),
        block_producer: Arc::new(block_producer),
        bft: Arc::new(bft),
        sync: Arc::new(sync),
        #[cfg(feature = "relayer")]
        relayer: Arc::new(relayer),
        #[cfg(feature = "p2p")]
        network_service: Arc::new(network_service),
    })
}
