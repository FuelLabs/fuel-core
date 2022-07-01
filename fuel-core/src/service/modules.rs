#![allow(clippy::let_unit_value)]
use crate::config::Config;
use crate::database::Database;
use anyhow::Result;
use fuel_core_interfaces::txpool::TxPoolDb;
use futures::future::join_all;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct Modules {
    pub txpool: Arc<fuel_txpool::Service>,
    pub block_importer: Arc<fuel_block_importer::Service>,
    pub block_producer: Arc<fuel_block_producer::Service>,
    pub bft: Arc<fuel_core_bft::Service>,
    pub sync: Arc<fuel_sync::Service>,
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
    let block_importer = fuel_block_importer::Service::new(&config.block_importer, db).await?;
    let block_producer = fuel_block_producer::Service::new(&config.block_producer, db).await?;
    let bft = fuel_core_bft::Service::new(&config.bft, db).await?;
    let sync = fuel_sync::Service::new(&config.sync).await?;
    // let mut relayer = FuelRelayer::new(FuelRelayerConfig::default());
    // let mut p2p = FuelP2P::new(FuelP2PConfig::default());
    let txpool = fuel_txpool::Service::new(
        Box::new(database.clone()) as Box<dyn TxPoolDb>,
        config.txpool.clone(),
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
