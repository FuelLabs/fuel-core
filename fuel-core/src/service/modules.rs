#![allow(clippy::let_unit_value)]
use crate::config::Config;
use crate::database::Database;
use anyhow::Result;
use fuel_core_interfaces::relayer::RelayerDb;
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
    pub relayer: Arc<fuel_relayer::Service>,
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

    // create builders
    let mut relayer_builder = fuel_relayer::ServiceBuilder::new();
    let mut txpool_builder = fuel_txpool::ServiceBuilder::new();

    // initiate fields for builders
    relayer_builder
        .config(config.relayer.clone())
        .db(Box::new(database.clone()) as Box<dyn RelayerDb>)
        .import_block_event(block_importer.subscribe())
        .private_key(
            hex::decode("c6bd905dcac2a0b1c43f574ab6933df14d7ceee0194902bce523ed054e8e798b")
                .unwrap(),
        );

    txpool_builder
        .config(config.txpool.clone())
        .db(Box::new(database.clone()) as Box<dyn TxPoolDb>)
        .import_block_event(block_importer.subscribe());

    let p2p_mpsc = ();
    let p2p_broadcast_consensus = ();
    let p2p_broadcast_block = ();

    block_importer.start().await;

    block_producer.start(txpool_builder.sender().clone()).await;
    bft.start(
        relayer_builder.sender().clone(),
        p2p_broadcast_consensus,
        block_producer.sender().clone(),
        block_importer.sender().clone(),
        block_importer.subscribe(),
    )
    .await;

    sync.start(
        p2p_broadcast_block,
        p2p_mpsc,
        relayer_builder.sender().clone(),
        bft.sender().clone(),
        block_importer.sender().clone(),
    )
    .await;

    // build services
    let relayer = relayer_builder.build()?;
    let txpool = txpool_builder.build()?;

    // start services
    if config.relayer.eth_client.is_some() {
        relayer.start().await?;
    }
    txpool.start().await?;

    Ok(Modules {
        txpool: Arc::new(txpool),
        block_importer: Arc::new(block_importer),
        block_producer: Arc::new(block_producer),
        bft: Arc::new(bft),
        sync: Arc::new(sync),
        relayer: Arc::new(relayer),
    })
}
