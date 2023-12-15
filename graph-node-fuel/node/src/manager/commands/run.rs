use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::chain::{
    connect_ethereum_networks, create_ethereum_networks_for_chain, create_firehose_networks,
    create_ipfs_clients,
};
use crate::config::Config;
use crate::manager::PanicSubscriptionManager;
use crate::store_builder::StoreBuilder;
use crate::MetricsContext;
use ethereum::chain::{EthereumAdapterSelector, EthereumBlockRefetcher, EthereumStreamBuilder};
use ethereum::{ProviderEthRpcMetrics, RuntimeAdapter as EthereumRuntimeAdapter};
use graph::anyhow::{bail, format_err};
use graph::blockchain::client::ChainClient;
use graph::blockchain::{BlockchainKind, BlockchainMap};
use graph::cheap_clone::CheapClone;
use graph::components::link_resolver::{ArweaveClient, FileSizeLimit};
use graph::components::store::{BlockStore as _, DeploymentLocator};
use graph::components::subgraph::Settings;
use graph::endpoint::EndpointMetrics;
use graph::env::EnvVars;
use graph::firehose::FirehoseEndpoints;
use graph::prelude::{
    anyhow, tokio, BlockNumber, DeploymentHash, LoggerFactory, NodeId, SubgraphAssignmentProvider,
    SubgraphCountMetric, SubgraphName, SubgraphRegistrar, SubgraphStore,
    SubgraphVersionSwitchingMode, ENV_VARS,
};
use graph::slog::{debug, info, Logger};
use graph_chain_ethereum as ethereum;
use graph_core::polling_monitor::{arweave_service, ipfs_service};
use graph_core::{
    LinkResolver, SubgraphAssignmentProvider as IpfsSubgraphAssignmentProvider,
    SubgraphInstanceManager, SubgraphRegistrar as IpfsSubgraphRegistrar,
};

fn locate(store: &dyn SubgraphStore, hash: &str) -> Result<DeploymentLocator, anyhow::Error> {
    let mut locators = store.locators(hash)?;
    match locators.len() {
        0 => bail!("could not find subgraph {hash} we just created"),
        1 => Ok(locators.pop().unwrap()),
        n => bail!("there are {n} subgraphs with hash {hash}"),
    }
}

pub async fn run(
    logger: Logger,
    store_builder: StoreBuilder,
    network_name: String,
    ipfs_url: Vec<String>,
    arweave_url: String,
    config: Config,
    metrics_ctx: MetricsContext,
    node_id: NodeId,
    subgraph: String,
    stop_block: BlockNumber,
) -> Result<(), anyhow::Error> {
    println!(
        "Run command: starting subgraph => {}, stop_block = {}",
        subgraph, stop_block
    );

    let env_vars = Arc::new(EnvVars::from_env().unwrap());
    let metrics_registry = metrics_ctx.registry.clone();
    let logger_factory = LoggerFactory::new(logger.clone(), None, metrics_ctx.registry.clone());

    // FIXME: Hard-coded IPFS config, take it from config file instead?
    let ipfs_clients: Vec<_> = create_ipfs_clients(&logger, &ipfs_url);
    let ipfs_client = ipfs_clients.first().cloned().expect("Missing IPFS client");
    let ipfs_service = ipfs_service(
        ipfs_client,
        env_vars.mappings.max_ipfs_file_bytes as u64,
        env_vars.mappings.ipfs_timeout,
        env_vars.mappings.ipfs_request_limit,
    );
    let arweave_resolver = Arc::new(ArweaveClient::new(
        logger.cheap_clone(),
        arweave_url.parse().expect("invalid arweave url"),
    ));
    let arweave_service = arweave_service(
        arweave_resolver.cheap_clone(),
        env_vars.mappings.ipfs_timeout,
        env_vars.mappings.ipfs_request_limit,
        match env_vars.mappings.max_ipfs_file_bytes {
            0 => FileSizeLimit::Unlimited,
            n => FileSizeLimit::MaxBytes(n as u64),
        },
    );

    let endpoint_metrics = Arc::new(EndpointMetrics::new(
        logger.clone(),
        &config.chains.providers(),
        metrics_registry.cheap_clone(),
    ));

    // Convert the clients into a link resolver. Since we want to get past
    // possible temporary DNS failures, make the resolver retry
    let link_resolver = Arc::new(LinkResolver::new(ipfs_clients, env_vars.cheap_clone()));

    let eth_rpc_metrics = Arc::new(ProviderEthRpcMetrics::new(metrics_registry.clone()));
    let eth_networks = create_ethereum_networks_for_chain(
        &logger,
        eth_rpc_metrics,
        &config,
        &network_name,
        endpoint_metrics.cheap_clone(),
    )
    .await
    .expect("Failed to parse Ethereum networks");
    let firehose_networks_by_kind =
        create_firehose_networks(logger.clone(), &config, endpoint_metrics);
    let firehose_networks = firehose_networks_by_kind.get(&BlockchainKind::Ethereum);
    let firehose_endpoints = firehose_networks
        .and_then(|v| v.networks.get(&network_name))
        .map_or_else(FirehoseEndpoints::new, |v| v.clone());

    let eth_adapters = match eth_networks.networks.get(&network_name) {
        Some(adapters) => adapters.clone(),
        None => {
            return Err(format_err!(
                "No ethereum adapters found, but required in this state of graphman run command"
            ))
        }
    };

    let eth_adapters2 = eth_adapters.clone();

    let (_, ethereum_idents) = connect_ethereum_networks(&logger, eth_networks).await?;
    // let (near_networks, near_idents) = connect_firehose_networks::<NearFirehoseHeaderOnlyBlock>(
    //     &logger,
    //     firehose_networks_by_kind
    //         .remove(&BlockchainKind::Near)
    //         .unwrap_or_else(|| FirehoseNetworks::new()),
    // )
    // .await;

    let chain_head_update_listener = store_builder.chain_head_update_listener();
    let network_identifiers = ethereum_idents.into_iter().collect();
    let network_store = store_builder.network_store(network_identifiers);

    let subgraph_store = network_store.subgraph_store();
    let chain_store = network_store
        .block_store()
        .chain_store(network_name.as_ref())
        .unwrap_or_else(|| panic!("No chain store for {}", &network_name));

    let client = Arc::new(ChainClient::new(firehose_endpoints, eth_adapters));

    let chain_config = config.chains.chains.get(&network_name).unwrap();
    let chain = ethereum::Chain::new(
        logger_factory.clone(),
        network_name.clone(),
        node_id.clone(),
        metrics_registry.clone(),
        chain_store.cheap_clone(),
        chain_store.cheap_clone(),
        client.clone(),
        chain_head_update_listener,
        Arc::new(EthereumStreamBuilder {}),
        Arc::new(EthereumBlockRefetcher {}),
        Arc::new(EthereumAdapterSelector::new(
            logger_factory.clone(),
            client,
            metrics_registry.clone(),
            chain_store.cheap_clone(),
        )),
        Arc::new(EthereumRuntimeAdapter {
            call_cache: chain_store.cheap_clone(),
            eth_adapters: Arc::new(eth_adapters2),
            chain_identifier: Arc::new(chain_store.chain_identifier.clone()),
        }),
        graph::env::ENV_VARS.reorg_threshold,
        chain_config.polling_interval,
        // We assume the tested chain is always ingestible for now
        true,
    );

    let mut blockchain_map = BlockchainMap::new();
    blockchain_map.insert(network_name.clone(), Arc::new(chain));

    let static_filters = ENV_VARS.experimental_static_filters;

    let sg_metrics = Arc::new(SubgraphCountMetric::new(metrics_registry.clone()));

    let blockchain_map = Arc::new(blockchain_map);
    let subgraph_instance_manager = SubgraphInstanceManager::new(
        &logger_factory,
        env_vars.cheap_clone(),
        subgraph_store.clone(),
        blockchain_map.clone(),
        sg_metrics.cheap_clone(),
        metrics_registry.clone(),
        link_resolver.cheap_clone(),
        ipfs_service,
        arweave_service,
        static_filters,
    );

    // Create IPFS-based subgraph provider
    let subgraph_provider = Arc::new(IpfsSubgraphAssignmentProvider::new(
        &logger_factory,
        link_resolver.cheap_clone(),
        subgraph_instance_manager,
        sg_metrics,
    ));

    let panicking_subscription_manager = Arc::new(PanicSubscriptionManager {});

    let subgraph_registrar = Arc::new(IpfsSubgraphRegistrar::new(
        &logger_factory,
        link_resolver.cheap_clone(),
        subgraph_provider.clone(),
        subgraph_store.clone(),
        panicking_subscription_manager,
        blockchain_map,
        node_id.clone(),
        SubgraphVersionSwitchingMode::Instant,
        Arc::new(Settings::default()),
    ));

    let (name, hash) = if subgraph.contains(':') {
        let mut split = subgraph.split(':');
        (split.next().unwrap(), split.next().unwrap().to_owned())
    } else {
        ("cli", subgraph)
    };

    let subgraph_name = SubgraphName::new(name)
        .expect("Subgraph name must contain only a-z, A-Z, 0-9, '-' and '_'");
    let subgraph_hash =
        DeploymentHash::new(hash.clone()).expect("Subgraph hash must be a valid IPFS hash");

    info!(&logger, "Creating subgraph {}", name);
    let create_result =
        SubgraphRegistrar::create_subgraph(subgraph_registrar.as_ref(), subgraph_name.clone())
            .await?;

    info!(
        &logger,
        "Looking up subgraph deployment {} (Deployment hash => {}, id => {})",
        name,
        subgraph_hash,
        create_result.id,
    );

    SubgraphRegistrar::create_subgraph_version(
        subgraph_registrar.as_ref(),
        subgraph_name.clone(),
        subgraph_hash.clone(),
        node_id.clone(),
        None,
        None,
        None,
        None,
    )
    .await?;

    let locator = locate(subgraph_store.as_ref(), &hash)?;

    SubgraphAssignmentProvider::start(subgraph_provider.as_ref(), locator, Some(stop_block))
        .await?;

    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let block_ptr = subgraph_store
            .least_block_ptr(&subgraph_hash)
            .await
            .unwrap()
            .unwrap();

        debug!(&logger, "subgraph block: {:?}", block_ptr);

        if block_ptr.number >= stop_block {
            info!(
                &logger,
                "subgraph now at block {}, reached stop block {}", block_ptr.number, stop_block
            );
            break;
        }
    }

    info!(&logger, "Removing subgraph {}", name);
    subgraph_store.clone().remove_subgraph(subgraph_name)?;

    if let Some(host) = metrics_ctx.prometheus_host {
        let mfs = metrics_ctx.prometheus.gather();
        let job_name = match metrics_ctx.job_name {
            Some(name) => name,
            None => "graphman run".into(),
        };

        tokio::task::spawn_blocking(move || {
            prometheus::push_metrics(&job_name, HashMap::new(), &host, mfs, None)
        })
        .await??;
    }

    Ok(())
}
