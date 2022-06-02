use super::Config;
use crate::{database::Database, tx_pool::TxPool};
use anyhow::Result;
use fuel_block_importer::{Config as FuelBlockImporterConfig, Service as FuelBlockImporterService};
use fuel_block_producer::{Config as FuelBlockProducerConfig, Service as FuelBlockProducerService};
use fuel_core_bft::{Config as FuelCoreBftConfig, Service as FuelCoreBftService};
use fuel_sync::{Config as FuelSyncConfig, Service as FuelSyncService};
use std::sync::Arc;

pub struct Modules {
    pub tx_pool: Arc<TxPool>,
    pub block_importer: Arc<FuelBlockImporterService>,
    pub block_producer: Arc<FuelBlockProducerService>,
    pub bft: Arc<FuelCoreBftService>,
    pub sync: Arc<FuelSyncService>,
}

pub async fn start_modules(config: &Config, database: &Database) -> Result<Modules> {
    let db = ();
    // initialize transaction pool
    let tx_pool = TxPool::new(database.clone(), config.clone());
    // Initialize and bind all components
    let mut block_importer =
        FuelBlockImporterService::new(&FuelBlockImporterConfig::default(), db).await?;
    let mut block_producer =
        FuelBlockProducerService::new(&FuelBlockProducerConfig::default(), db).await?;
    let mut bft = FuelCoreBftService::new(&FuelCoreBftConfig::default(), db).await?;
    let mut sync = FuelSyncService::new(&FuelSyncConfig::default()).await?;
    // let mut relayer = FuelRelayer::new(FuelRelayerConfig::default());
    // let mut p2p = FuelP2P::new(FuelP2PConfig::default());
    // let mut txpool = FuelTxpool::new(FuelTxpoolConfig::default());

    let txpool_mpsc = ();
    let p2p_mpsc = ();
    let p2p_broadcast_consensus = ();
    let p2p_broadcast_block = ();
    let relayer_mpsc = ();

    block_importer.start().await;
    block_producer.start(txpool_mpsc).await;
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
        tx_pool: Arc::new(tx_pool),
        block_importer: Arc::new(block_importer),
        block_producer: Arc::new(block_producer),
        bft: Arc::new(bft),
        sync: Arc::new(sync),
    })
}
