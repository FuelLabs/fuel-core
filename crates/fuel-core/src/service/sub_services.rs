#![allow(clippy::let_unit_value)]
use super::adapters::P2PAdapter;
use crate::{
    chain_config::BlockProduction,
    database::Database,
    fuel_core_graphql_api::Config as GraphQLConfig,
    service::{
        adapters::{
            BlockImportAdapter,
            BlockProducerAdapter,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            TxPoolAdapter,
        },
        Config,
        SharedState,
        SubServices,
    },
};
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
pub type GraphQL = crate::fuel_core_graphql_api::service::Service;

pub fn init_sub_services(
    config: &Config,
    database: &Database,
) -> anyhow::Result<(SubServices, SharedState)> {
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
    let network = {
        let p2p_db = database.clone();

        let genesis = p2p_db.get_genesis()?;
        let p2p_config = config.p2p.clone().init(genesis)?;

        fuel_core_p2p::service::new_service(p2p_config, p2p_db)
    };

    #[cfg(feature = "p2p")]
    let p2p_adapter = P2PAdapter::new(network.shared.clone());
    #[cfg(not(feature = "p2p"))]
    let p2p_adapter = P2PAdapter::new();

    let p2p_adapter = p2p_adapter;

    let importer_adapter = BlockImportAdapter::new(block_import_tx);

    let txpool = fuel_core_txpool::new_service(
        config.txpool.clone(),
        database.clone(),
        TxStatusChange::new(100),
        importer_adapter.clone(),
        p2p_adapter,
    );

    let executor = ExecutorAdapter {
        database: database.clone(),
        config: config.clone(),
    };

    // restrict the max number of concurrent dry runs to the number of CPUs
    // as execution in the worst case will be CPU bound rather than I/O bound.
    let max_dry_run_concurrency = num_cpus::get();
    let block_producer = Arc::new(fuel_core_producer::Producer {
        config: config.block_producer.clone(),
        db: database.clone(),
        txpool: Box::new(TxPoolAdapter::new(txpool.shared.clone())),
        executor: Arc::new(executor.clone()),
        relayer: Box::new(MaybeRelayerAdapter {
            database: database.clone(),
            #[cfg(feature = "relayer")]
            relayer_synced: relayer.as_ref().map(|r| r.shared.clone()),
        }),
        lock: Mutex::new(()),
        dry_run_semaphore: Semaphore::new(max_dry_run_concurrency),
    });

    let poa = match &config.chain_conf.block_production {
        BlockProduction::ProofOfAuthority { trigger } => fuel_core_poa::new_service(
            fuel_core_poa::Config {
                trigger: *trigger,
                block_gas_limit: config.chain_conf.block_gas_limit,
                signing_key: config.consensus_key.clone(),
                metrics: false,
            },
            TxPoolAdapter::new(txpool.shared.clone()),
            // TODO: Pass Importer
            importer_adapter.tx,
            BlockProducerAdapter {
                block_producer: block_producer.clone(),
            },
            database.clone(),
        ),
    };

    let graph_ql = crate::fuel_core_graphql_api::service::new_service(
        GraphQLConfig {
            addr: config.addr,
            utxo_validation: config.utxo_validation,
            manual_blocks_enabled: config.manual_blocks_enabled,
            vm_backtrace: config.vm.backtrace,
            min_gas_price: config.txpool.min_gas_price,
            max_tx: config.txpool.max_tx,
            max_depth: config.txpool.max_depth,
            transaction_parameters: config.chain_conf.transaction_parameters,
            consensus_key: config.consensus_key.clone(),
        },
        database.clone(),
        block_producer,
        txpool.clone(),
        executor,
    )?;

    let shared = SharedState {
        txpool: txpool.shared.clone(),
        #[cfg(feature = "p2p")]
        network: network.shared.clone(),
        #[cfg(feature = "relayer")]
        relayer: relayer.as_ref().map(|r| r.shared.clone()),
        graph_ql: graph_ql.shared.clone(),
    };

    #[allow(unused_mut)]
    // `FuelService` starts and shutdowns all sub-services in the `services` order
    let mut services: SubServices = vec![
        // GraphQL should be shutdown first, so let's start it first.
        Box::new(graph_ql),
        Box::new(poa),
        Box::new(txpool),
    ];

    #[cfg(feature = "relayer")]
    if let Some(relayer) = relayer {
        services.push(Box::new(relayer));
    }

    #[cfg(feature = "p2p")]
    {
        services.push(Box::new(network));
    }

    Ok((services, shared))
}
