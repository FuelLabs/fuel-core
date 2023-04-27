#![allow(clippy::let_unit_value)]
use super::adapters::P2PAdapter;

use crate::{
    database::Database,
    fuel_core_graphql_api::Config as GraphQLConfig,
    schema::build_schema,
    service::{
        adapters::{
            BlockImporterAdapter,
            BlockProducerAdapter,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            PoAAdapter,
            TxPoolAdapter,
            VerifierAdapter,
        },
        Config,
        SharedState,
        SubServices,
    },
};
use fuel_core_poa::Trigger;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type PoAService =
    fuel_core_poa::Service<TxPoolAdapter, BlockProducerAdapter, BlockImporterAdapter>;
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
    let last_block = database.get_current_block()?.ok_or(anyhow::anyhow!(
        "The blockchain is not initialized with any block"
    ))?;
    #[cfg(feature = "relayer")]
    let relayer_service = if config.relayer.eth_client.is_some() {
        Some(fuel_core_relayer::new_service(
            database.clone(),
            config.relayer.clone(),
        )?)
    } else {
        None
    };

    let relayer_adapter = MaybeRelayerAdapter {
        database: database.clone(),
        #[cfg(feature = "relayer")]
        relayer_synced: relayer_service.as_ref().map(|r| r.shared.clone()),
        #[cfg(feature = "relayer")]
        da_deploy_height: config.relayer.da_deploy_height,
    };

    let executor = ExecutorAdapter {
        relayer: relayer_adapter.clone(),
        config: config.clone(),
    };

    let verifier =
        VerifierAdapter::new(config, database.clone(), relayer_adapter.clone());

    let importer_adapter = BlockImporterAdapter::new(
        config.block_importer.clone(),
        database.clone(),
        executor.clone(),
        verifier.clone(),
    );

    #[cfg(feature = "p2p")]
    let mut network = {
        if let Some(config) = config.p2p.clone() {
            let p2p_db = database.clone();
            let genesis = p2p_db.get_genesis()?;
            let p2p_config = config.init(genesis)?;

            Some(fuel_core_p2p::service::new_service(
                p2p_config,
                p2p_db,
                importer_adapter.clone(),
            ))
        } else {
            None
        }
    };

    #[cfg(feature = "p2p")]
    let p2p_adapter =
        P2PAdapter::new(network.as_ref().map(|network| network.shared.clone()));
    #[cfg(not(feature = "p2p"))]
    let p2p_adapter = P2PAdapter::new();

    let p2p_adapter = p2p_adapter;

    let txpool = fuel_core_txpool::new_service(
        config.txpool.clone(),
        database.clone(),
        importer_adapter.clone(),
        p2p_adapter.clone(),
    );
    let tx_pool_adapter = TxPoolAdapter::new(txpool.shared.clone());

    let block_producer = fuel_core_producer::Producer {
        config: config.block_producer.clone(),
        db: database.clone(),
        txpool: Box::new(tx_pool_adapter.clone()),
        executor: Arc::new(executor),
        relayer: Box::new(relayer_adapter),
        lock: Mutex::new(()),
    };
    let producer_adapter = BlockProducerAdapter::new(block_producer);

    let poa_config: fuel_core_poa::Config = config.try_into()?;
    let production_enabled =
        !matches!(poa_config.trigger, Trigger::Never) || config.manual_blocks_enabled;
    let poa = (production_enabled).then(|| {
        fuel_core_poa::new_service(
            last_block.header(),
            poa_config,
            tx_pool_adapter.clone(),
            producer_adapter.clone(),
            importer_adapter.clone(),
        )
    });
    let poa_adapter = PoAAdapter::new(poa.as_ref().map(|service| service.shared.clone()));

    #[cfg(feature = "p2p")]
    let sync = (!production_enabled)
        .then(|| {
            fuel_core_sync::service::new_service(
                *last_block.header().height(),
                p2p_adapter,
                importer_adapter.clone(),
                verifier,
                config.sync,
            )
        })
        .transpose()?;

    // TODO: Figure out on how to move it into `fuel-core-graphql-api`.
    let schema = {
        #[cfg(feature = "dap")]
        {
            crate::schema::dap::init(
                build_schema(),
                config.chain_conf.transaction_parameters,
                config.chain_conf.gas_costs.clone(),
            )
            .data(database.clone())
        }
        #[cfg(not(feature = "dap"))]
        {
            build_schema()
        }
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
        schema,
        Box::new(database.clone()),
        Box::new(tx_pool_adapter),
        Box::new(producer_adapter),
        Box::new(poa_adapter),
    )?;

    let shared = SharedState {
        txpool: txpool.shared.clone(),
        #[cfg(feature = "p2p")]
        network: network.as_ref().map(|n| n.shared.clone()),
        #[cfg(feature = "relayer")]
        relayer: relayer_service.as_ref().map(|r| r.shared.clone()),
        graph_ql: graph_ql.shared.clone(),
        block_importer: importer_adapter,
        #[cfg(feature = "test-helpers")]
        config: config.clone(),
    };

    #[allow(unused_mut)]
    // `FuelService` starts and shutdowns all sub-services in the `services` order
    let mut services: SubServices = vec![
        // GraphQL should be shutdown first, so let's start it first.
        Box::new(graph_ql),
        Box::new(txpool),
    ];

    if let Some(poa) = poa {
        services.push(Box::new(poa));
    }

    #[cfg(feature = "relayer")]
    if let Some(relayer) = relayer_service {
        services.push(Box::new(relayer));
    }

    #[cfg(feature = "p2p")]
    {
        if let Some(network) = network.take() {
            services.push(Box::new(network));
            if let Some(sync) = sync {
                services.push(Box::new(sync));
            }
        }
    }

    Ok((services, shared))
}
