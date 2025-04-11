#![allow(clippy::let_unit_value)]

use std::sync::Arc;

use tokio::sync::Mutex;

use fuel_core_gas_price_service::v1::{
    algorithm::AlgorithmV1,
    da_source_service::block_committer_costs::{
        BlockCommitterDaBlockCosts,
        BlockCommitterHttpApi,
    },
    metadata::V1AlgorithmConfig,
    uninitialized_task::new_gas_price_service_v1,
};

use fuel_core_poa::Trigger;
use fuel_core_storage::{
    self,
    transactional::AtomicView,
};
#[cfg(feature = "relayer")]
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use fuel_core_types::signer::SignMode;

use fuel_core_compression_service::service::new_service as new_compression_service;

#[cfg(feature = "relayer")]
use crate::relayer::Config as RelayerConfig;

#[cfg(feature = "p2p")]
use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::{
    key_generator::Ed25519KeyGenerator,
    trigger::TimeBasedTrigger,
    tx_receiver::PreconfirmationsReceiver,
};

use super::{
    DbType,
    adapters::{
        FuelBlockSigner,
        P2PAdapter,
        TxStatusManagerAdapter,
        compression_adapters::{
            CompressionBlockImporterAdapter,
            CompressionServiceAdapter,
        },
    },
    config::DaCompressionMode,
    genesis::create_genesis_block,
};
use crate::{
    combined_database::CombinedDatabase,
    database::Database,
    fuel_core_graphql_api::{
        self,
        Config as GraphQLConfig,
    },
    graphql_api::worker_service::{
        self,
    },
    schema::build_schema,
    service::{
        Config,
        SharedState,
        SubServices,
        adapters::{
            BlockImporterAdapter,
            BlockProducerAdapter,
            ChainStateInfoProvider,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            PoAAdapter,
            PreconfirmationSender,
            SharedMemoryPool,
            SystemTime,
            TxPoolAdapter,
            UniversalGasPriceProvider,
            VerifierAdapter,
            chain_state_info_provider,
            consensus_module::poa::InDirectoryPredefinedBlocks,
            fuel_gas_price_provider::FuelGasPriceProvider,
            graphql_api::GraphQLBlockImporter,
            import_result_provider::ImportResultProvider,
            ready_signal::ReadySignal,
            tx_status_manager::ConsensusConfigProtocolPublicKey,
        },
    },
};

pub type PoAService = fuel_core_poa::Service<
    BlockProducerAdapter,
    BlockImporterAdapter,
    SignMode,
    InDirectoryPredefinedBlocks,
    SystemTime,
    ReadySignal,
>;
#[cfg(feature = "p2p")]
pub type P2PService = fuel_core_p2p::service::Service<Database, TxPoolAdapter>;
pub type TxPoolSharedState = fuel_core_txpool::SharedState;
pub type BlockProducerService = fuel_core_producer::block_producer::Producer<
    Database,
    TxPoolAdapter,
    ExecutorAdapter,
    FuelGasPriceProvider<AlgorithmV1, u32, u64>,
    ChainStateInfoProvider,
>;
pub type GraphQL = fuel_core_graphql_api::api_service::Service;

// TODO: Add to consensus params https://github.com/FuelLabs/fuel-vm/issues/888
pub const DEFAULT_GAS_PRICE_CHANGE_PERCENT: u16 = 10;

pub fn init_sub_services(
    config: &Config,
    database: CombinedDatabase,
    block_production_ready_signal: ReadySignal,
) -> anyhow::Result<(SubServices, SharedState)> {
    let chain_config = config.snapshot_reader.chain_config();
    let chain_id = chain_config.consensus_parameters.chain_id();
    let chain_name = chain_config.chain_name.clone();
    let on_chain_view = database.on_chain().latest_view()?;
    let (new_txs_updater, new_txs_watcher) = tokio::sync::watch::channel(());
    #[cfg(feature = "p2p")]
    let (preconfirmation_sender, preconfirmation_receiver) =
        tokio::sync::mpsc::channel(1024);
    #[cfg(not(feature = "p2p"))]
    let (preconfirmation_sender, _) = tokio::sync::mpsc::channel(1024);

    let genesis_block = on_chain_view
        .genesis_block()?
        .unwrap_or(create_genesis_block(config).compress(&chain_id));
    let last_block_header = on_chain_view
        .get_current_block()?
        .map(|block| block.header().clone())
        .unwrap_or(genesis_block.header().clone());

    let last_height = *last_block_header.height();

    if config.historical_execution
        && config.combined_db_config.database_type != DbType::RocksDb
    {
        return Err(anyhow::anyhow!(
            "Historical execution is only supported with RocksDB"
        ));
    }

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

    let protocol_pubkey =
        ConsensusConfigProtocolPublicKey::new(chain_config.consensus.clone());

    let tx_status_manager = fuel_core_tx_status_manager::new_service(
        p2p_adapter.clone(),
        config.tx_status_manager.clone(),
        protocol_pubkey,
    );
    let tx_status_manager_adapter =
        TxStatusManagerAdapter::new(tx_status_manager.shared.clone());
    let preconfirmation_sender = PreconfirmationSender::new(
        preconfirmation_sender,
        tx_status_manager_adapter.clone(),
    );

    let upgradable_executor_config = fuel_core_upgradable_executor::config::Config {
        forbid_fake_coins_default: config.utxo_validation,
        native_executor_version: config.native_executor_version,
        allow_historical_execution: config.historical_execution,
    };
    let executor = ExecutorAdapter::new(
        database.on_chain().clone(),
        database.relayer().clone(),
        upgradable_executor_config,
        new_txs_watcher,
        preconfirmation_sender.clone(),
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

    let chain_state_info_provider_service = chain_state_info_provider::new_service(
        database.on_chain().clone(),
        &importer_adapter,
    );
    let chain_state_info_provider =
        ChainStateInfoProvider::new(chain_state_info_provider_service.shared.clone());

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
        relayer_database: database.relayer().clone(),
        #[cfg(feature = "relayer")]
        da_deploy_height: config.relayer.as_ref().map_or(
            DaBlockHeight(RelayerConfig::DEFAULT_DA_DEPLOY_HEIGHT),
            |config| config.da_deploy_height,
        ),
    };

    let genesis_block_height = *genesis_block.header().height();
    let settings = chain_state_info_provider.clone();
    let block_stream = importer_adapter.events_shared_result();

    let committer_api =
        BlockCommitterHttpApi::new(config.gas_price_config.da_committer_url.clone());
    let da_source = BlockCommitterDaBlockCosts::new(committer_api);
    let v1_config = V1AlgorithmConfig::from(config.clone());

    let gas_price_service_v1 = new_gas_price_service_v1(
        v1_config,
        genesis_block_height,
        settings,
        block_stream,
        database.gas_price().clone(),
        da_source,
        database.on_chain().clone(),
    )?;
    let (gas_price_algo, latest_gas_price) = gas_price_service_v1.shared.clone();
    let universal_gas_price_provider = UniversalGasPriceProvider::new_from_inner(
        latest_gas_price,
        DEFAULT_GAS_PRICE_CHANGE_PERCENT,
    );

    let producer_gas_price_provider = FuelGasPriceProvider::new(
        gas_price_algo.clone(),
        universal_gas_price_provider.clone(),
    );

    let txpool = fuel_core_txpool::new_service(
        chain_id,
        config.txpool.clone(),
        p2p_adapter.clone(),
        importer_adapter.clone(),
        database.on_chain().clone(),
        chain_state_info_provider.clone(),
        last_height,
        universal_gas_price_provider.clone(),
        executor.clone(),
        new_txs_updater,
        preconfirmation_sender,
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
        gas_price_provider: producer_gas_price_provider.clone(),
        chain_state_info_provider: chain_state_info_provider.clone(),
    };
    let producer_adapter = BlockProducerAdapter::new(block_producer);

    let poa_config: fuel_core_poa::Config = config.into();
    let mut production_enabled = !matches!(poa_config.trigger, Trigger::Never);

    if !production_enabled
        && config.debug
        && !matches!(poa_config.signer, SignMode::Unavailable)
    {
        production_enabled = true;
        tracing::info!("Enabled manual block production because of `debug` flag");
    }

    let signer = FuelBlockSigner::new(config.consensus_signer.clone());

    #[cfg(feature = "shared-sequencer")]
    let shared_sequencer = {
        let config = config.shared_sequencer.clone();

        fuel_core_shared_sequencer::service::new_service(
            importer_adapter.clone(),
            config,
            Arc::new(signer.clone()),
        )?
    };

    let predefined_blocks =
        InDirectoryPredefinedBlocks::new(config.predefined_blocks_path.clone());

    #[cfg(feature = "p2p")]
    let config_preconfirmation: fuel_core_poa::pre_confirmation_signature_service::config::Config =
        config.into();

    #[cfg(feature = "p2p")]
    let pre_confirmation_service = production_enabled
        .then(|| {
            fuel_core_poa::pre_confirmation_signature_service::new_service(
                config_preconfirmation.clone(),
                PreconfirmationsReceiver::new(preconfirmation_receiver),
                p2p_adapter.clone(),
                signer.clone(),
                Ed25519KeyGenerator,
                TimeBasedTrigger::new(
                    SystemTime,
                    config_preconfirmation.key_rotation_interval,
                    config_preconfirmation.key_expiration_interval,
                ),
            )
        })
        .transpose()?;

    let poa = production_enabled.then(|| {
        fuel_core_poa::new_service(
            &last_block_header,
            poa_config,
            tx_pool_adapter.clone(),
            producer_adapter.clone(),
            importer_adapter.clone(),
            p2p_adapter.clone(),
            Arc::new(signer),
            predefined_blocks,
            SystemTime,
            block_production_ready_signal,
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

    // we allow the consumers of the database access even
    // when the compression service is disabled
    let compression_service_adapter =
        CompressionServiceAdapter::new(database.compression().clone());

    let compression_importer_adapter = CompressionBlockImporterAdapter::new(
        importer_adapter.clone(),
        import_result_provider.clone(),
    );

    let compression_service = match &config.da_compression {
        DaCompressionMode::Disabled => None,
        DaCompressionMode::Enabled(cfg) => Some(
            new_compression_service(
                compression_importer_adapter,
                database.compression().clone(),
                cfg.clone(),
                database.on_chain().clone(),
            )
            .map_err(|e| anyhow::anyhow!(e))?,
        ),
    };

    let schema = crate::schema::dap::init(build_schema(), config.debug)
        .data(database.on_chain().clone());

    let graphql_block_importer =
        GraphQLBlockImporter::new(importer_adapter.clone(), import_result_provider);
    let graphql_worker_context = worker_service::Context {
        tx_status_manager: tx_status_manager_adapter.clone(),
        block_importer: graphql_block_importer,
        on_chain_database: database.on_chain().clone(),
        off_chain_database: database.off_chain().clone(),
        continue_on_error: config.continue_on_error,
        consensus_parameters: &chain_config.consensus_parameters,
    };
    let graphql_worker =
        fuel_core_graphql_api::worker_service::new_service(graphql_worker_context)?;

    let graphql_block_height_subscription_handle = graphql_worker.shared.clone();

    let graphql_config = GraphQLConfig {
        config: config.graphql_config.clone(),
        utxo_validation: config.utxo_validation,
        debug: config.debug,
        historical_execution: config.historical_execution,
        max_tx: config.txpool.pool_limits.max_txs,
        max_gas: config.txpool.pool_limits.max_gas,
        max_size: config.txpool.pool_limits.max_bytes_size,
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
        Box::new(tx_status_manager_adapter.clone()),
        Box::new(producer_adapter),
        Box::new(poa_adapter.clone()),
        Box::new(p2p_adapter),
        Box::new(universal_gas_price_provider),
        Box::new(chain_state_info_provider),
        SharedMemoryPool::new(config.memory_pool_size),
        graphql_block_height_subscription_handle,
        Box::new(compression_service_adapter),
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
        tx_status_manager: tx_status_manager_adapter,
        compression: compression_service.as_ref().map(|c| c.shared.clone()),
    };

    #[allow(unused_mut)]
    // `FuelService` starts and shutdowns all sub-services in the `services` order
    let mut services: SubServices = vec![
        Box::new(gas_price_service_v1),
        Box::new(txpool),
        Box::new(chain_state_info_provider_service),
    ];

    #[cfg(feature = "relayer")]
    if let Some(relayer) = relayer_service {
        services.push(Box::new(relayer));
    }

    #[cfg(feature = "p2p")]
    {
        if let Some(network) = network.take() {
            services.push(Box::new(network));
            services.push(Box::new(sync));
            if let Some(pre_confirmation_service) = pre_confirmation_service {
                services.push(Box::new(pre_confirmation_service));
            }
        }
    }
    #[cfg(feature = "shared-sequencer")]
    services.push(Box::new(shared_sequencer));

    services.push(Box::new(graph_ql));
    services.push(Box::new(graphql_worker));
    services.push(Box::new(tx_status_manager));

    if let Some(compression_service) = compression_service {
        services.push(Box::new(compression_service));
    }

    // always make sure that the block producer is inserted last
    if let Some(poa) = poa {
        services.push(Box::new(poa));
    }

    Ok((services, shared))
}
