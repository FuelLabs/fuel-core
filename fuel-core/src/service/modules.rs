use super::Config;
use crate::database::Database;
use anyhow::Result;
use fuel_block_importer::{Config as FuelBlockImporterConfig, Service as FuelBlockImporterService};
use fuel_block_producer::{Config as FuelBlockProducerConfig, Service as FuelBlockProducerService};
use fuel_core_bft::{Config as FuelCoreBftConfig, Service as FuelCoreBftService};
use fuel_core_interfaces::txpool::TxPoolDb;
use fuel_sync::{Config as FuelSyncConfig, Service as FuelSyncService};
use fuel_txpool::Service as FuelTxPoolService;
use futures::future::join_all;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct Modules {
    pub txpool: Arc<FuelTxPoolService>,
    pub block_importer: Arc<FuelBlockImporterService>,
    pub block_producer: Arc<FuelBlockProducerService>,
    pub bft: Arc<FuelCoreBftService>,
    pub sync: Arc<FuelSyncService>,
}

impl Modules {
    pub async fn stop(&self) {
        let stops: Vec<JoinHandle<()>> = vec![
            self.txpool.stop().await,
            self.block_importer.stop().await,
            self.block_producer.stop().await,
            self.bft.stop().await,
            self.sync.stop().await,
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
    let block_importer =
        FuelBlockImporterService::new(&FuelBlockImporterConfig::default(), db).await?;
    let block_producer =
        FuelBlockProducerService::new(&FuelBlockProducerConfig::default(), db).await?;
    let bft = FuelCoreBftService::new(&FuelCoreBftConfig::default(), db).await?;
    let sync = FuelSyncService::new(&FuelSyncConfig::default()).await?;
    // let mut relayer = FuelRelayer::new(FuelRelayerConfig::default());
    // let mut p2p = FuelP2P::new(FuelP2PConfig::default());
    let txpool = FuelTxPoolService::new(
        Box::new(database.clone()) as Box<dyn TxPoolDb>,
        config.tx_pool_config.clone(),
    )?;

    let p2p_mpsc = ();
    let p2p_broadcast_consensus = ();
    let p2p_broadcast_block = ();
    let relayer_mpsc = ();

    block_importer.start().await;
    txpool.start(block_importer.subscribe()).await;
    block_producer.start(txpool.sender().clone()).await;
    bft.start(
        relayer_mpsc,
        p2p_broadcast_consensus,
        block_producer.sender().clone(),
        block_importer.sender().clone(),
        block_importer.subscribe(),
    )
    .await;

    sync.start(
        p2p_broadcast_block,
        p2p_mpsc,
        relayer_mpsc,
        bft.sender().clone(),
        block_importer.sender().clone(),
    )
    .await;

    Ok(Modules {
        txpool: Arc::new(txpool),
        block_importer: Arc::new(block_importer),
        block_producer: Arc::new(block_producer),
        bft: Arc::new(bft),
        sync: Arc::new(sync),
    })
}
