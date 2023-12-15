use clap::Parser as _;
use ethereum::chain::{EthereumAdapterSelector, EthereumBlockRefetcher, EthereumStreamBuilder};
use ethereum::{BlockIngestor, EthereumNetworks, RuntimeAdapter};
use git_testament::{git_testament, render_testament};
use graph::blockchain::client::ChainClient;
use graph_chain_ethereum::codec::HeaderOnlyBlock;

use graph::blockchain::{
    BasicBlockchainBuilder, Blockchain, BlockchainBuilder, BlockchainKind, BlockchainMap,
    ChainIdentifier,
};
use graph::components::link_resolver::{ArweaveClient, FileSizeLimit};
use graph::components::store::BlockStore;
use graph::components::subgraph::Settings;
use graph::data::graphql::load_manager::LoadManager;
use graph::endpoint::EndpointMetrics;
use graph::env::EnvVars;
use graph::firehose::{FirehoseEndpoints, FirehoseNetworks};
use graph::log::logger;
use graph::prelude::{IndexNodeServer as _, *};
use graph::prometheus::Registry;
use graph::url::Url;
use graph_chain_arweave::{self as arweave, Block as ArweaveBlock};
use graph_chain_cosmos::{self as cosmos, Block as CosmosFirehoseBlock};
use graph_chain_ethereum as ethereum;
use graph_chain_near::{self as near, HeaderOnlyBlock as NearFirehoseHeaderOnlyBlock};
use graph_chain_starknet::{self as starknet, Block as StarknetBlock};
use graph_chain_substreams as substreams;
use graph_core::polling_monitor::{arweave_service, ipfs_service};
use graph_core::{
    LinkResolver, SubgraphAssignmentProvider as IpfsSubgraphAssignmentProvider,
    SubgraphInstanceManager, SubgraphRegistrar as IpfsSubgraphRegistrar,
};
use graph_graphql::prelude::GraphQlRunner;
use graph_node::chain::{
    connect_ethereum_networks, connect_firehose_networks, create_all_ethereum_networks,
    create_firehose_networks, create_ipfs_clients, create_substreams_networks,
};
use graph_node::config::Config;
use graph_node::opt;
use graph_node::store_builder::StoreBuilder;
use graph_server_http::GraphQLServer as GraphQLQueryServer;
use graph_server_index_node::IndexNodeServer;
use graph_server_json_rpc::JsonRpcServer;
use graph_server_metrics::PrometheusMetricsServer;
use graph_server_websocket::SubscriptionServer as GraphQLSubscriptionServer;
use graph_store_postgres::{register_jobs as register_store_jobs, ChainHeadUpdateListener, Store};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

git_testament!(TESTAMENT);

fn read_expensive_queries(
    logger: &Logger,
    expensive_queries_filename: String,
) -> Result<Vec<Arc<q::Document>>, std::io::Error> {
    // A file with a list of expensive queries, one query per line
    // Attempts to run these queries will return a
    // QueryExecutionError::TooExpensive to clients
    let path = Path::new(&expensive_queries_filename);
    let mut queries = Vec::new();
    if path.exists() {
        info!(
            logger,
            "Reading expensive queries file: {}", expensive_queries_filename
        );
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let query = graphql_parser::parse_query(&line)
                .map_err(|e| {
                    let msg = format!(
                        "invalid GraphQL query in {}: {}\n{}",
                        expensive_queries_filename, e, line
                    );
                    std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
                })?
                .into_static();
            queries.push(Arc::new(query));
        }
    } else {
        warn!(
            logger,
            "Expensive queries file not set to a valid file: {}", expensive_queries_filename
        );
    }
    Ok(queries)
}

macro_rules! collect_ingestors {
    ($acc:ident, $logger:ident, $($chain:ident),+) => {
        $(
        $chain.iter().for_each(|(network_name, chain)| {
            let logger = $logger.new(o!("network_name" => network_name.clone()));
            match chain.block_ingestor() {
                Ok(ingestor) =>{
                    info!(logger, "Started block ingestor");
                    $acc.push(ingestor);
                }
                Err(err) => error!(&logger,
                    "Failed to create block ingestor {}",err),
            }
        });
        )+
    };
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let env_vars = Arc::new(EnvVars::from_env().unwrap());
    let opt = opt::Opt::parse();

    // Set up logger
    let logger = logger(opt.debug);

    // Log version information
    info!(
        logger,
        "Graph Node version: {}",
        render_testament!(TESTAMENT)
    );

    if !graph_server_index_node::PoiProtection::from_env(&ENV_VARS).is_active() {
        warn!(
            logger,
            "GRAPH_POI_ACCESS_TOKEN not set; might leak POIs to the public via GraphQL"
        );
    }

    let config = match Config::load(&logger, &opt.clone().into()) {
        Err(e) => {
            eprintln!("configuration error: {}", e);
            std::process::exit(1);
        }
        Ok(config) => config,
    };

    let subgraph_settings = match env_vars.subgraph_settings {
        Some(ref path) => {
            info!(logger, "Reading subgraph configuration file `{}`", path);
            match Settings::from_file(path) {
                Ok(rules) => rules,
                Err(e) => {
                    eprintln!("configuration error in subgraph settings {}: {}", path, e);
                    std::process::exit(1);
                }
            }
        }
        None => Settings::default(),
    };

    if opt.check_config {
        match config.to_json() {
            Ok(txt) => println!("{}", txt),
            Err(e) => eprintln!("error serializing config: {}", e),
        }
        eprintln!("Successfully validated configuration");
        std::process::exit(0);
    }

    let node_id = NodeId::new(opt.node_id.clone())
        .expect("Node ID must be between 1 and 63 characters in length");
    let query_only = config.query_only(&node_id);

    // Obtain subgraph related command-line arguments
    let subgraph = opt.subgraph.clone();

    // Obtain ports to use for the GraphQL server(s)
    let http_port = opt.http_port;
    let ws_port = opt.ws_port;

    // Obtain JSON-RPC server port
    let json_rpc_port = opt.admin_port;

    // Obtain index node server port
    let index_node_port = opt.index_node_port;

    // Obtain metrics server port
    let metrics_port = opt.metrics_port;

    // Obtain the fork base URL
    let fork_base = match &opt.fork_base {
        Some(url) => {
            // Make sure the endpoint ends with a terminating slash.
            let url = if !url.ends_with('/') {
                let mut url = url.clone();
                url.push('/');
                Url::parse(&url)
            } else {
                Url::parse(url)
            };

            Some(url.expect("Failed to parse the fork base URL"))
        }
        None => {
            warn!(
                logger,
                "No fork base URL specified, subgraph forking is disabled"
            );
            None
        }
    };

    info!(logger, "Starting up");

    // Optionally, identify the Elasticsearch logging configuration
    let elastic_config = opt
        .elasticsearch_url
        .clone()
        .map(|endpoint| ElasticLoggingConfig {
            endpoint,
            username: opt.elasticsearch_user.clone(),
            password: opt.elasticsearch_password.clone(),
            client: reqwest::Client::new(),
        });

    // Set up Prometheus registry
    let prometheus_registry = Arc::new(Registry::new());
    let metrics_registry = Arc::new(MetricsRegistry::new(
        logger.clone(),
        prometheus_registry.clone(),
    ));

    // Create a component and subgraph logger factory
    let logger_factory =
        LoggerFactory::new(logger.clone(), elastic_config, metrics_registry.clone());

    // Try to create IPFS clients for each URL specified in `--ipfs`
    let ipfs_clients: Vec<_> = create_ipfs_clients(&logger, &opt.ipfs);
    let ipfs_client = ipfs_clients.first().cloned().expect("Missing IPFS client");
    let ipfs_service = ipfs_service(
        ipfs_client,
        ENV_VARS.mappings.max_ipfs_file_bytes as u64,
        ENV_VARS.mappings.ipfs_timeout,
        ENV_VARS.mappings.ipfs_request_limit,
    );
    let arweave_resolver = Arc::new(ArweaveClient::new(
        logger.cheap_clone(),
        opt.arweave
            .parse()
            .expect("unable to parse arweave gateway address"),
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

    // Convert the clients into a link resolver. Since we want to get past
    // possible temporary DNS failures, make the resolver retry
    let link_resolver = Arc::new(LinkResolver::new(ipfs_clients, env_vars.cheap_clone()));
    let mut metrics_server =
        PrometheusMetricsServer::new(&logger_factory, prometheus_registry.clone());

    let endpoint_metrics = Arc::new(EndpointMetrics::new(
        logger.clone(),
        &config.chains.providers(),
        metrics_registry.cheap_clone(),
    ));

    // Ethereum clients; query nodes ignore all ethereum clients and never
    // connect to them directly
    let eth_networks = if query_only {
        EthereumNetworks::new(endpoint_metrics.cheap_clone())
    } else {
        create_all_ethereum_networks(
            logger.clone(),
            metrics_registry.clone(),
            &config,
            endpoint_metrics.cheap_clone(),
        )
        .await
        .expect("Failed to parse Ethereum networks")
    };

    let mut firehose_networks_by_kind = if query_only {
        BTreeMap::new()
    } else {
        create_firehose_networks(logger.clone(), &config, endpoint_metrics.cheap_clone())
    };

    let mut substreams_networks_by_kind = if query_only {
        BTreeMap::new()
    } else {
        create_substreams_networks(logger.clone(), &config, endpoint_metrics.clone())
    };

    let graphql_metrics_registry = metrics_registry.clone();

    let contention_logger = logger.clone();

    // TODO: make option loadable from configuration TOML and environment:
    let expensive_queries =
        read_expensive_queries(&logger, opt.expensive_queries_filename).unwrap();

    let store_builder = StoreBuilder::new(
        &logger,
        &node_id,
        &config,
        fork_base,
        metrics_registry.cheap_clone(),
    )
    .await;

    let launch_services = |logger: Logger, env_vars: Arc<EnvVars>| async move {
        let subscription_manager = store_builder.subscription_manager();
        let chain_head_update_listener = store_builder.chain_head_update_listener();
        let primary_pool = store_builder.primary_pool();

        // To support the ethereum block ingestor, ethereum networks are referenced both by the
        // `blockchain_map` and `ethereum_chains`. Future chains should be referred to only in
        // `blockchain_map`.
        let mut blockchain_map = BlockchainMap::new();

        // Unwraps: `connect_ethereum_networks` and `connect_firehose_networks` only fail if
        // mismatching chain identifiers are returned for a same network, which indicates a serious
        // inconsistency between providers.
        let (arweave_networks, arweave_idents) = connect_firehose_networks::<ArweaveBlock>(
            &logger,
            firehose_networks_by_kind
                .remove(&BlockchainKind::Arweave)
                .unwrap_or_else(FirehoseNetworks::new),
        )
        .await
        .unwrap();

        // This only has idents for chains with rpc adapters.
        let (eth_networks, ethereum_idents) = connect_ethereum_networks(&logger, eth_networks)
            .await
            .unwrap();

        let (eth_firehose_only_networks, eth_firehose_only_idents) =
            connect_firehose_networks::<HeaderOnlyBlock>(
                &logger,
                firehose_networks_by_kind
                    .remove(&BlockchainKind::Ethereum)
                    .unwrap_or_else(FirehoseNetworks::new),
            )
            .await
            .unwrap();

        let (near_networks, near_idents) =
            connect_firehose_networks::<NearFirehoseHeaderOnlyBlock>(
                &logger,
                firehose_networks_by_kind
                    .remove(&BlockchainKind::Near)
                    .unwrap_or_else(FirehoseNetworks::new),
            )
            .await
            .unwrap();

        let (cosmos_networks, cosmos_idents) = connect_firehose_networks::<CosmosFirehoseBlock>(
            &logger,
            firehose_networks_by_kind
                .remove(&BlockchainKind::Cosmos)
                .unwrap_or_else(FirehoseNetworks::new),
        )
        .await
        .unwrap();

        let substreams_networks = substreams_networks_by_kind
            .remove(&BlockchainKind::Substreams)
            .unwrap_or_else(FirehoseNetworks::new);

        let (starknet_networks, starknet_idents) = connect_firehose_networks::<StarknetBlock>(
            &logger,
            firehose_networks_by_kind
                .remove(&BlockchainKind::Starknet)
                .unwrap_or_else(FirehoseNetworks::new),
        )
        .await
        .unwrap();

        let substream_idents = substreams_networks
            .networks
            .keys()
            .map(|name| {
                (
                    name.clone(),
                    ChainIdentifier {
                        net_version: name.to_string(),
                        genesis_block_hash: BlockHash::default(),
                    },
                )
            })
            .collect::<BTreeMap<String, ChainIdentifier>>();

        // Note that both `eth_firehose_only_idents` and `ethereum_idents` contain Ethereum
        // networks. If the same network is configured in both RPC and Firehose, the RPC ident takes
        // precedence. This is necessary because Firehose endpoints currently have no `net_version`.
        // See also: firehose-no-net-version.
        let mut network_identifiers = eth_firehose_only_idents;
        network_identifiers.extend(ethereum_idents);
        network_identifiers.extend(arweave_idents);
        network_identifiers.extend(near_idents);
        network_identifiers.extend(cosmos_idents);
        network_identifiers.extend(substream_idents);
        network_identifiers.extend(starknet_idents);

        let network_store = store_builder.network_store(network_identifiers);

        let arweave_chains = networks_as_chains::<arweave::Chain>(
            &env_vars,
            &mut blockchain_map,
            &logger,
            &arweave_networks,
            substreams_networks_by_kind.get(&BlockchainKind::Arweave),
            network_store.as_ref(),
            &logger_factory,
            metrics_registry.clone(),
        );

        let eth_firehose_only_networks = if eth_firehose_only_networks.networks.len() == 0 {
            None
        } else {
            Some(&eth_firehose_only_networks)
        };

        if !opt.disable_block_ingestor && eth_networks.networks.len() != 0 {
            let eth_network_names = Vec::from_iter(eth_networks.networks.keys());
            let fh_only = match eth_firehose_only_networks {
                Some(firehose_only) => Some(Vec::from_iter(firehose_only.networks.keys())),
                None => None,
            };
            network_store
                .block_store()
                .cleanup_ethereum_shallow_blocks(eth_network_names, fh_only)
                .unwrap();
        }

        let ethereum_chains = ethereum_networks_as_chains(
            &mut blockchain_map,
            &logger,
            &config,
            node_id.clone(),
            metrics_registry.clone(),
            eth_firehose_only_networks,
            substreams_networks_by_kind.get(&BlockchainKind::Ethereum),
            &eth_networks,
            network_store.as_ref(),
            chain_head_update_listener,
            &logger_factory,
            metrics_registry.clone(),
        );

        let near_chains = networks_as_chains::<near::Chain>(
            &env_vars,
            &mut blockchain_map,
            &logger,
            &near_networks,
            substreams_networks_by_kind.get(&BlockchainKind::Near),
            network_store.as_ref(),
            &logger_factory,
            metrics_registry.clone(),
        );

        let cosmos_chains = networks_as_chains::<cosmos::Chain>(
            &env_vars,
            &mut blockchain_map,
            &logger,
            &cosmos_networks,
            substreams_networks_by_kind.get(&BlockchainKind::Cosmos),
            network_store.as_ref(),
            &logger_factory,
            metrics_registry.clone(),
        );

        let substreams_chains = networks_as_chains::<substreams::Chain>(
            &env_vars,
            &mut blockchain_map,
            &logger,
            &substreams_networks,
            None,
            network_store.as_ref(),
            &logger_factory,
            metrics_registry.clone(),
        );

        let starknet_chains = networks_as_chains::<starknet::Chain>(
            &env_vars,
            &mut blockchain_map,
            &logger,
            &starknet_networks,
            substreams_networks_by_kind.get(&BlockchainKind::Starknet),
            network_store.as_ref(),
            &logger_factory,
            metrics_registry.clone(),
        );

        let blockchain_map = Arc::new(blockchain_map);

        let shards: Vec<_> = config.stores.keys().cloned().collect();
        let load_manager = Arc::new(LoadManager::new(
            &logger,
            shards,
            expensive_queries,
            metrics_registry.clone(),
        ));
        let graphql_runner = Arc::new(GraphQlRunner::new(
            &logger,
            network_store.clone(),
            subscription_manager.clone(),
            load_manager,
            graphql_metrics_registry,
        ));
        let mut graphql_server =
            GraphQLQueryServer::new(&logger_factory, graphql_runner.clone(), node_id.clone());
        let subscription_server =
            GraphQLSubscriptionServer::new(&logger, graphql_runner.clone(), network_store.clone());

        let mut index_node_server = IndexNodeServer::new(
            &logger_factory,
            blockchain_map.clone(),
            graphql_runner.clone(),
            network_store.clone(),
            link_resolver.clone(),
        );

        if !opt.disable_block_ingestor {
            let logger = logger.clone();
            let mut ingestors: Vec<Box<dyn BlockIngestor>> = vec![];
            collect_ingestors!(
                ingestors,
                logger,
                ethereum_chains,
                arweave_chains,
                near_chains,
                cosmos_chains,
                substreams_chains,
                starknet_chains
            );

            ingestors.into_iter().for_each(|ingestor| {
                let logger = logger.clone();
                info!(logger,"Starting block ingestor for network";"network_name" => &ingestor.network_name());

                graph::spawn(ingestor.run());
            });

            // Start a task runner
            let mut job_runner = graph::util::jobs::Runner::new(&logger);
            register_store_jobs(
                &mut job_runner,
                network_store.clone(),
                primary_pool,
                metrics_registry.clone(),
            );
            graph::spawn_blocking(job_runner.start());
        }
        let static_filters = ENV_VARS.experimental_static_filters;

        let sg_count = Arc::new(SubgraphCountMetric::new(metrics_registry.cheap_clone()));

        let subgraph_instance_manager = SubgraphInstanceManager::new(
            &logger_factory,
            env_vars.cheap_clone(),
            network_store.subgraph_store(),
            blockchain_map.cheap_clone(),
            sg_count.cheap_clone(),
            metrics_registry.clone(),
            link_resolver.clone(),
            ipfs_service,
            arweave_service,
            static_filters,
        );

        // Create IPFS-based subgraph provider
        let subgraph_provider = IpfsSubgraphAssignmentProvider::new(
            &logger_factory,
            link_resolver.clone(),
            subgraph_instance_manager,
            sg_count,
        );

        // Check version switching mode environment variable
        let version_switching_mode = ENV_VARS.subgraph_version_switching_mode;

        // Create named subgraph provider for resolving subgraph name->ID mappings
        let subgraph_registrar = Arc::new(IpfsSubgraphRegistrar::new(
            &logger_factory,
            link_resolver,
            Arc::new(subgraph_provider),
            network_store.subgraph_store(),
            subscription_manager,
            blockchain_map,
            node_id.clone(),
            version_switching_mode,
            Arc::new(subgraph_settings),
        ));
        graph::spawn(
            subgraph_registrar
                .start()
                .map_err(|e| panic!("failed to initialize subgraph provider {}", e))
                .compat(),
        );

        // Start admin JSON-RPC server.
        let json_rpc_server = JsonRpcServer::serve(
            json_rpc_port,
            http_port,
            ws_port,
            subgraph_registrar.clone(),
            node_id.clone(),
            logger.clone(),
        )
        .await
        .expect("failed to start JSON-RPC admin server");

        // Let the server run forever.
        std::mem::forget(json_rpc_server);

        // Add the CLI subgraph with a REST request to the admin server.
        if let Some(subgraph) = subgraph {
            let (name, hash) = if subgraph.contains(':') {
                let mut split = subgraph.split(':');
                (split.next().unwrap(), split.next().unwrap().to_owned())
            } else {
                ("cli", subgraph)
            };

            let name = SubgraphName::new(name)
                .expect("Subgraph name must contain only a-z, A-Z, 0-9, '-' and '_'");
            let subgraph_id =
                DeploymentHash::new(hash).expect("Subgraph hash must be a valid IPFS hash");
            let debug_fork = opt
                .debug_fork
                .map(DeploymentHash::new)
                .map(|h| h.expect("Debug fork hash must be a valid IPFS hash"));
            let start_block = opt
                .start_block
                .map(|block| {
                    let mut split = block.split(':');
                    (
                        // BlockHash
                        split.next().unwrap().to_owned(),
                        // BlockNumber
                        split.next().unwrap().parse::<i64>().unwrap(),
                    )
                })
                .map(|(hash, number)| BlockPtr::try_from((hash.as_str(), number)))
                .map(Result::unwrap);

            graph::spawn(
                async move {
                    subgraph_registrar.create_subgraph(name.clone()).await?;
                    subgraph_registrar
                        .create_subgraph_version(
                            name,
                            subgraph_id,
                            node_id,
                            debug_fork,
                            start_block,
                            None,
                            None,
                        )
                        .await
                }
                .map_err(|e| panic!("Failed to deploy subgraph from `--subgraph` flag: {}", e)),
            );
        }

        // Serve GraphQL queries over HTTP
        graph::spawn(
            graphql_server
                .serve(http_port, ws_port)
                .expect("Failed to start GraphQL query server")
                .compat(),
        );

        // Serve GraphQL subscriptions over WebSockets
        graph::spawn(subscription_server.serve(ws_port));

        // Run the index node server
        graph::spawn(
            index_node_server
                .serve(index_node_port)
                .expect("Failed to start index node server")
                .compat(),
        );

        graph::spawn(async move {
            metrics_server
                .serve(metrics_port)
                .await
                .expect("Failed to start metrics server")
        });
    };

    graph::spawn(launch_services(logger.clone(), env_vars.cheap_clone()));

    // Periodically check for contention in the tokio threadpool. First spawn a
    // task that simply responds to "ping" requests. Then spawn a separate
    // thread to periodically ping it and check responsiveness.
    let (ping_send, mut ping_receive) = mpsc::channel::<std::sync::mpsc::SyncSender<()>>(1);
    graph::spawn(async move {
        while let Some(pong_send) = ping_receive.recv().await {
            let _ = pong_send.clone().send(());
        }
        panic!("ping sender dropped");
    });
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let (pong_send, pong_receive) = std::sync::mpsc::sync_channel(1);
        if futures::executor::block_on(ping_send.clone().send(pong_send)).is_err() {
            debug!(contention_logger, "Shutting down contention checker thread");
            break;
        }
        let mut timeout = Duration::from_millis(10);
        while pong_receive.recv_timeout(timeout) == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            debug!(contention_logger, "Possible contention in tokio threadpool";
                                     "timeout_ms" => timeout.as_millis(),
                                     "code" => LogCode::TokioContention);
            if timeout < ENV_VARS.kill_if_unresponsive_timeout {
                timeout *= 10;
            } else if ENV_VARS.kill_if_unresponsive {
                // The node is unresponsive, kill it in hopes it will be restarted.
                crit!(contention_logger, "Node is unresponsive, killing process");
                std::process::abort()
            }
        }
    });

    futures::future::pending::<()>().await;
}

/// Return the hashmap of chains and also add them to `blockchain_map`.
fn networks_as_chains<C>(
    config: &Arc<EnvVars>,
    blockchain_map: &mut BlockchainMap,
    logger: &Logger,
    firehose_networks: &FirehoseNetworks,
    substreams_networks: Option<&FirehoseNetworks>,
    store: &Store,
    logger_factory: &LoggerFactory,
    metrics_registry: Arc<MetricsRegistry>,
) -> HashMap<String, Arc<C>>
where
    C: Blockchain,
    BasicBlockchainBuilder: BlockchainBuilder<C>,
{
    let chains: Vec<_> = firehose_networks
        .networks
        .iter()
        .filter_map(|(chain_id, endpoints)| {
            store
                .block_store()
                .chain_store(chain_id)
                .map(|chain_store| (chain_id, chain_store, endpoints))
                .or_else(|| {
                    error!(
                        logger,
                        "No store configured for {} chain {}; ignoring this chain",
                        C::KIND,
                        chain_id
                    );
                    None
                })
        })
        .map(|(chain_id, chain_store, endpoints)| {
            (
                chain_id.clone(),
                Arc::new(
                    BasicBlockchainBuilder {
                        logger_factory: logger_factory.clone(),
                        name: chain_id.clone(),
                        chain_store,
                        firehose_endpoints: endpoints.clone(),
                        metrics_registry: metrics_registry.clone(),
                    }
                    .build(config),
                ),
            )
        })
        .collect();

    for (chain_id, chain) in chains.iter() {
        blockchain_map.insert::<C>(chain_id.clone(), chain.clone())
    }

    if let Some(substreams_networks) = substreams_networks {
        for (network_name, firehose_endpoints) in substreams_networks.networks.iter() {
            let chain_store = blockchain_map
                .get::<C>(network_name.clone())
                .expect(&format!(
                    "{} requires an rpc or firehose endpoint defined",
                    network_name
                ))
                .chain_store();

            blockchain_map.insert::<substreams::Chain>(
                network_name.clone(),
                Arc::new(substreams::Chain::new(
                    logger_factory.clone(),
                    firehose_endpoints.clone(),
                    metrics_registry.clone(),
                    chain_store,
                    Arc::new(substreams::BlockStreamBuilder::new()),
                )),
            );
        }
    }

    HashMap::from_iter(chains)
}

/// Return the hashmap of ethereum chains and also add them to `blockchain_map`.
fn ethereum_networks_as_chains(
    blockchain_map: &mut BlockchainMap,
    logger: &Logger,
    config: &Config,
    node_id: NodeId,
    registry: Arc<MetricsRegistry>,
    firehose_networks: Option<&FirehoseNetworks>,
    substreams_networks: Option<&FirehoseNetworks>,
    eth_networks: &EthereumNetworks,
    store: &Store,
    chain_head_update_listener: Arc<ChainHeadUpdateListener>,
    logger_factory: &LoggerFactory,
    metrics_registry: Arc<MetricsRegistry>,
) -> HashMap<String, Arc<ethereum::Chain>> {
    let chains: Vec<_> = eth_networks
        .networks
        .iter()
        .filter_map(|(network_name, eth_adapters)| {
            store
                .block_store()
                .chain_store(network_name)
                .map(|chain_store| {
                    let is_ingestible = chain_store.is_ingestible();
                    (network_name, eth_adapters, chain_store, is_ingestible)
                })
                .or_else(|| {
                    error!(
                        logger,
                        "No store configured for Ethereum chain {}; ignoring this chain",
                        network_name
                    );
                    None
                })
        })
        .map(|(network_name, eth_adapters, chain_store, is_ingestible)| {
            let firehose_endpoints = firehose_networks
                .and_then(|v| v.networks.get(network_name))
                .map_or_else(FirehoseEndpoints::new, |v| v.clone());

            let client = Arc::new(ChainClient::<graph_chain_ethereum::Chain>::new(
                firehose_endpoints,
                eth_adapters.clone(),
            ));
            let adapter_selector = EthereumAdapterSelector::new(
                logger_factory.clone(),
                client.clone(),
                registry.clone(),
                chain_store.clone(),
            );

            let runtime_adapter = Arc::new(RuntimeAdapter {
                eth_adapters: Arc::new(eth_adapters.clone()),
                call_cache: chain_store.cheap_clone(),
                chain_identifier: Arc::new(chain_store.chain_identifier.clone()),
            });

            let chain_config = config.chains.chains.get(network_name).unwrap();
            let chain = ethereum::Chain::new(
                logger_factory.clone(),
                network_name.clone(),
                node_id.clone(),
                registry.clone(),
                chain_store.cheap_clone(),
                chain_store,
                client,
                chain_head_update_listener.clone(),
                Arc::new(EthereumStreamBuilder {}),
                Arc::new(EthereumBlockRefetcher {}),
                Arc::new(adapter_selector),
                runtime_adapter,
                ENV_VARS.reorg_threshold,
                chain_config.polling_interval,
                is_ingestible,
            );
            (network_name.clone(), Arc::new(chain))
        })
        .collect();

    for (network_name, chain) in chains.iter().cloned() {
        blockchain_map.insert::<graph_chain_ethereum::Chain>(network_name, chain)
    }

    if let Some(substreams_networks) = substreams_networks {
        for (network_name, firehose_endpoints) in substreams_networks.networks.iter() {
            let chain_store = blockchain_map
                .get::<graph_chain_ethereum::Chain>(network_name.clone())
                .expect("any substreams endpoint needs an rpc or firehose chain defined")
                .chain_store();

            blockchain_map.insert::<substreams::Chain>(
                network_name.clone(),
                Arc::new(substreams::Chain::new(
                    logger_factory.clone(),
                    firehose_endpoints.clone(),
                    metrics_registry.clone(),
                    chain_store,
                    Arc::new(substreams::BlockStreamBuilder::new()),
                )),
            );
        }
    }

    HashMap::from_iter(chains)
}
