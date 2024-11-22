#![allow(clippy::let_unit_value)]

use super::{
    adapters::{
        FuelBlockSigner,
        P2PAdapter,
    },
    genesis::create_genesis_block,
};
#[cfg(feature = "relayer")]
use crate::relayer::Config as RelayerConfig;
use crate::{
    combined_database::CombinedDatabase,
    database::Database,
    fuel_core_graphql_api,
    fuel_core_graphql_api::Config as GraphQLConfig,
    schema::build_schema,
    service::{
        adapters::{
            consensus_module::poa::InDirectoryPredefinedBlocks,
            consensus_parameters_provider,
            fuel_gas_price_provider::FuelGasPriceProvider,
            graphql_api::GraphQLBlockImporter,
            import_result_provider::ImportResultProvider,
            BlockImporterAdapter,
            BlockProducerAdapter,
            ConsensusParametersProvider,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            PoAAdapter,
            SharedMemoryPool,
            SystemTime,
            TxPoolAdapter,
            VerifierAdapter,
        },
        Config,
        SharedState,
        SubServices,
    },
};
use fuel_core_gas_price_service::v0::uninitialized_task::{
    new_gas_price_service_v0,
    AlgorithmV0,
};
use fuel_core_poa::Trigger;
use fuel_core_shared_sequencer::ports::Signer;
use fuel_core_storage::{
    self,
    structured_storage::StructuredStorage,
    transactional::AtomicView,
};
#[cfg(feature = "relayer")]
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use fuel_core_types::signer::SignMode;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type PoAService = fuel_core_poa::Service<
    TxPoolAdapter,
    BlockProducerAdapter,
    BlockImporterAdapter,
    SignMode,
    InDirectoryPredefinedBlocks,
    SystemTime,
>;
#[cfg(feature = "p2p")]
pub type P2PService = fuel_core_p2p::service::Service<Database, TxPoolAdapter>;
pub type TxPoolSharedState = fuel_core_txpool::SharedState;
pub type BlockProducerService = fuel_core_producer::block_producer::Producer<
    Database,
    TxPoolAdapter,
    ExecutorAdapter,
    FuelGasPriceProvider<AlgorithmV0>,
    ConsensusParametersProvider,
>;

pub type GraphQL = fuel_core_graphql_api::api_service::Service;

pub fn init_sub_services(
    config: &Config,
    database: CombinedDatabase,
) -> anyhow::Result<(SubServices, SharedState)> {
    let chain_config = config.snapshot_reader.chain_config();
    let chain_id = chain_config.consensus_parameters.chain_id();
    let chain_name = chain_config.chain_name.clone();
    let on_chain_view = database.on_chain().latest_view()?;

    let genesis_block = on_chain_view
        .genesis_block()?
        .unwrap_or(create_genesis_block(config).compress(&chain_id));
    let last_block_header = on_chain_view
        .get_current_block()?
        .map(|block| block.header().clone())
        .unwrap_or(genesis_block.header().clone());

    let last_height = *last_block_header.height();

    let executor = ExecutorAdapter::new(
        database.on_chain().clone(),
        database.relayer().clone(),
        fuel_core_upgradable_executor::config::Config {
            backtrace: config.vm.backtrace,
            utxo_validation_default: config.utxo_validation,
            native_executor_version: config.native_executor_version,
        },
    );
    let import_result_provider =
        ImportResultProvider::new(database.on_chain().clone(), executor.clone());

    let verifier = VerifierAdapter::new(
        &genesis_block,
        chain_config.consensus.clone(),
        database.on_chain().clone(),
    );

    let importer_adapter = BlockImporterAdapter::new(
        chain_id,
        config.block_importer.clone(),
        database.on_chain().clone(),
        executor.clone(),
        verifier.clone(),
    );

    let consensus_parameters_provider_service =
        consensus_parameters_provider::new_service(
            database.on_chain().clone(),
            &importer_adapter,
        );
    let consensus_parameters_provider = ConsensusParametersProvider::new(
        consensus_parameters_provider_service.shared.clone(),
    );

    #[cfg(feature = "relayer")]
    let relayer_service = if let Some(config) = &config.relayer {
        Some(fuel_core_relayer::new_service(
            database.relayer().clone(),
            config.clone(),
        )?)
    } else {
        None
    };

    let relayer_adapter = MaybeRelayerAdapter {
        #[cfg(feature = "relayer")]
        relayer_synced: relayer_service.as_ref().map(|r| r.shared.clone()),
        #[cfg(feature = "relayer")]
        da_deploy_height: config.relayer.as_ref().map_or(
            DaBlockHeight(RelayerConfig::DEFAULT_DA_DEPLOY_HEIGHT),
            |config| config.da_deploy_height,
        ),
    };

    #[cfg(feature = "p2p")]
    let p2p_externals = config
        .p2p
        .clone()
        .map(fuel_core_p2p::service::build_shared_state);

    #[cfg(feature = "p2p")]
    let p2p_adapter = {
        use crate::service::adapters::PeerReportConfig;

        // Hardcoded for now, but left here to be configurable in the future.
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1340
        let peer_report_config = PeerReportConfig {
            successful_block_import: 5.,
            missing_block_headers: -100.,
            bad_block_header: -100.,
            missing_transactions: -100.,
            invalid_transactions: -100.,
        };
        P2PAdapter::new(
            p2p_externals.as_ref().map(|ext| ext.0.clone()),
            peer_report_config,
        )
    };

    #[cfg(not(feature = "p2p"))]
    let p2p_adapter = P2PAdapter::new();

    let genesis_block_height = *genesis_block.header().height();
    let settings = consensus_parameters_provider.clone();
    let block_stream = importer_adapter.events_shared_result();
    let metadata = StructuredStorage::new(database.gas_price().clone());

    let gas_price_service_v0 = new_gas_price_service_v0(
        config.clone().into(),
        genesis_block_height,
        settings,
        block_stream,
        database.gas_price().clone(),
        metadata,
        database.on_chain().clone(),
    )?;

    let gas_price_provider =
        FuelGasPriceProvider::new(gas_price_service_v0.shared.clone());
    let txpool = fuel_core_txpool::new_service(
        chain_id,
        config.txpool.clone(),
        p2p_adapter.clone(),
        importer_adapter.clone(),
        database.on_chain().clone(),
        consensus_parameters_provider.clone(),
        last_height,
        gas_price_provider.clone(),
        executor.clone(),
    );
    let tx_pool_adapter = TxPoolAdapter::new(txpool.shared.clone());

    #[cfg(feature = "p2p")]
    let mut network = config.p2p.clone().zip(p2p_externals).map(
        |(p2p_config, (shared_state, request_receiver))| {
            fuel_core_p2p::service::new_service(
                chain_id,
                last_height,
                p2p_config,
                shared_state,
                request_receiver,
                database.on_chain().clone(),
                importer_adapter.clone(),
                tx_pool_adapter.clone(),
            )
        },
    );

    let block_producer = fuel_core_producer::Producer {
        config: config.block_producer.clone(),
        view_provider: database.on_chain().clone(),
        txpool: tx_pool_adapter.clone(),
        executor: Arc::new(executor.clone()),
        relayer: Box::new(relayer_adapter.clone()),
        lock: Mutex::new(()),
        gas_price_provider: gas_price_provider.clone(),
        consensus_parameters_provider: consensus_parameters_provider.clone(),
    };
    let producer_adapter = BlockProducerAdapter::new(block_producer);

    let poa_config: fuel_core_poa::Config = config.into();
    let mut production_enabled = !matches!(poa_config.trigger, Trigger::Never);

    if !production_enabled && config.debug {
        production_enabled = true;
        tracing::info!("Enabled manual block production because of `debug` flag");
    }

    let signer = Arc::new(FuelBlockSigner::new(config.consensus_signer.clone()));

    #[cfg(feature = "shared-sequencer")]
    let shared_sequencer = {
        let config = config.shared_sequencer.clone();

        if signer.is_available() {
            let cosmos_public_address = config.sender_account_id(signer.as_ref())?;

            tracing::info!(
                "Shared sequencer uses account ID: {}",
                cosmos_public_address
            );
        }

        config.enabled.then(|| {
            fuel_core_shared_sequencer::service::new_service(
                importer_adapter.clone(),
                config,
                signer.clone(),
            )
        })
    };

    let predefined_blocks =
        InDirectoryPredefinedBlocks::new(config.predefined_blocks_path.clone());
    let poa = production_enabled.then(|| {
        fuel_core_poa::new_service(
            &last_block_header,
            poa_config,
            tx_pool_adapter.clone(),
            producer_adapter.clone(),
            importer_adapter.clone(),
            p2p_adapter.clone(),
            signer,
            predefined_blocks,
            SystemTime,
        )
    });
    let poa_adapter = PoAAdapter::new(poa.as_ref().map(|service| service.shared.clone()));

    #[cfg(feature = "p2p")]
    let sync = fuel_core_sync::service::new_service(
        last_height,
        p2p_adapter.clone(),
        importer_adapter.clone(),
        super::adapters::ConsensusAdapter::new(
            verifier.clone(),
            config.relayer_consensus_config.clone(),
            relayer_adapter,
        ),
        config.sync,
    )?;

    let schema = crate::schema::dap::init(build_schema(), config.debug)
        .data(database.on_chain().clone());

    let graphql_block_importer =
        GraphQLBlockImporter::new(importer_adapter.clone(), import_result_provider);
    let graphql_worker = fuel_core_graphql_api::worker_service::new_service(
        tx_pool_adapter.clone(),
        graphql_block_importer,
        database.on_chain().clone(),
        database.off_chain().clone(),
        chain_id,
        config.da_compression.clone(),
        config.continue_on_error,
    );

    let graphql_config = GraphQLConfig {
        config: config.graphql_config.clone(),
        utxo_validation: config.utxo_validation,
        debug: config.debug,
        vm_backtrace: config.vm.backtrace,
        max_tx: config.txpool.pool_limits.max_txs,
        max_txpool_dependency_chain_length: config.txpool.max_txs_chain_count,
        chain_name,
    };

    let graph_ql = fuel_core_graphql_api::api_service::new_service(
        *genesis_block.header().height(),
        graphql_config,
        schema,
        database.on_chain().clone(),
        database.off_chain().clone(),
        Box::new(tx_pool_adapter),
        Box::new(producer_adapter),
        Box::new(poa_adapter.clone()),
        Box::new(p2p_adapter),
        Box::new(gas_price_provider),
        Box::new(consensus_parameters_provider),
        SharedMemoryPool::new(config.memory_pool_size),
    )?;

    let shared = SharedState {
        poa_adapter,
        txpool_shared_state: txpool.shared.clone(),
        #[cfg(feature = "p2p")]
        network: network.as_ref().map(|n| n.shared.clone()),
        #[cfg(feature = "relayer")]
        relayer: relayer_service.as_ref().map(|r| r.shared.clone()),
        graph_ql: graph_ql.shared.clone(),
        database,
        block_importer: importer_adapter,
        executor,
        config: config.clone(),
    };

    #[allow(unused_mut)]
    // `FuelService` starts and shutdowns all sub-services in the `services` order
    let mut services: SubServices = vec![
        Box::new(gas_price_service_v0),
        Box::new(txpool),
        Box::new(consensus_parameters_provider_service),
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
            services.push(Box::new(sync));
        }
    }
    #[cfg(feature = "shared-sequencer")]
    {
        if let Some(shared_sequencer) = shared_sequencer {
            services.push(Box::new(shared_sequencer));
        }
    }

    services.push(Box::new(graph_ql));
    services.push(Box::new(graphql_worker));

    Ok((services, shared))
}
