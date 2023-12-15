pub mod ethereum;
pub mod substreams;

use std::marker::PhantomData;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Error;
use async_stream::stream;
use futures::{Stream, StreamExt};
use graph::blockchain::block_stream::{
    BlockRefetcher, BlockStream, BlockStreamBuilder, BlockStreamEvent, BlockWithTriggers,
    FirehoseCursor,
};
use graph::blockchain::{
    Block, BlockHash, BlockPtr, Blockchain, BlockchainMap, ChainIdentifier, RuntimeAdapter,
    TriggersAdapter, TriggersAdapterSelector,
};
use graph::cheap_clone::CheapClone;
use graph::components::link_resolver::{ArweaveClient, ArweaveResolver, FileSizeLimit};
use graph::components::metrics::MetricsRegistry;
use graph::components::store::{BlockStore, DeploymentLocator};
use graph::components::subgraph::Settings;
use graph::data::graphql::load_manager::LoadManager;
use graph::data::query::{Query, QueryTarget};
use graph::data::subgraph::schema::{SubgraphError, SubgraphHealth};
use graph::endpoint::EndpointMetrics;
use graph::env::EnvVars;
use graph::firehose::{FirehoseEndpoint, FirehoseEndpoints, SubgraphLimit};
use graph::ipfs_client::IpfsClient;
use graph::prelude::ethabi::ethereum_types::H256;
use graph::prelude::serde_json::{self, json};
use graph::prelude::{
    async_trait, lazy_static, r, ApiVersion, BigInt, BlockNumber, DeploymentHash,
    GraphQlRunner as _, LoggerFactory, NodeId, QueryError, SubgraphAssignmentProvider,
    SubgraphCountMetric, SubgraphName, SubgraphRegistrar, SubgraphStore as _,
    SubgraphVersionSwitchingMode, TriggerProcessor,
};
use graph::schema::InputSchema;
use graph_chain_ethereum::Chain;
use graph_core::polling_monitor::{arweave_service, ipfs_service};
use graph_core::{
    LinkResolver, SubgraphAssignmentProvider as IpfsSubgraphAssignmentProvider,
    SubgraphInstanceManager, SubgraphRegistrar as IpfsSubgraphRegistrar, SubgraphTriggerProcessor,
};
use graph_node::manager::PanicSubscriptionManager;
use graph_node::{config::Config, store_builder::StoreBuilder};
use graph_runtime_wasm::RuntimeHostBuilder;
use graph_server_index_node::IndexNodeService;
use graph_store_postgres::{ChainHeadUpdateListener, ChainStore, Store, SubgraphStore};
use serde::Deserialize;
use slog::{crit, info, o, Discard, Logger};
use std::env::VarError;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::fs::read_to_string;

const NODE_ID: &str = "default";

pub fn test_ptr(n: BlockNumber) -> BlockPtr {
    test_ptr_reorged(n, 0)
}

// Set n as the low bits and `reorg_n` as the high bits of the hash.
pub fn test_ptr_reorged(n: BlockNumber, reorg_n: u32) -> BlockPtr {
    let mut hash = H256::from_low_u64_be(n as u64);
    hash[0..4].copy_from_slice(&reorg_n.to_be_bytes());
    BlockPtr {
        hash: hash.into(),
        number: n,
    }
}

type GraphQlRunner = graph_graphql::prelude::GraphQlRunner<Store, PanicSubscriptionManager>;

struct CommonChainConfig {
    logger_factory: LoggerFactory,
    mock_registry: Arc<MetricsRegistry>,
    chain_store: Arc<ChainStore>,
    firehose_endpoints: FirehoseEndpoints,
    node_id: NodeId,
}

impl CommonChainConfig {
    async fn new(test_name: &str, stores: &Stores) -> Self {
        let logger = test_logger(test_name);
        let mock_registry = Arc::new(MetricsRegistry::mock());
        let logger_factory = LoggerFactory::new(logger.cheap_clone(), None, mock_registry.clone());
        let chain_store = stores.chain_store.cheap_clone();
        let node_id = NodeId::new(NODE_ID).unwrap();

        let firehose_endpoints: FirehoseEndpoints = vec![Arc::new(FirehoseEndpoint::new(
            "",
            "https://example.com",
            None,
            true,
            false,
            SubgraphLimit::Unlimited,
            Arc::new(EndpointMetrics::mock()),
        ))]
        .into();

        Self {
            logger_factory,
            mock_registry,
            chain_store,
            firehose_endpoints,
            node_id,
        }
    }
}

pub struct TestChain<C: Blockchain> {
    pub chain: Arc<C>,
    pub block_stream_builder: Arc<MutexBlockStreamBuilder<C>>,
}

impl TestChainTrait<Chain> for TestChain<Chain> {
    fn set_block_stream(&self, blocks: Vec<BlockWithTriggers<Chain>>) {
        let static_block_stream = Arc::new(StaticStreamBuilder { chain: blocks });
        *self.block_stream_builder.0.lock().unwrap() = static_block_stream;
    }

    fn chain(&self) -> Arc<Chain> {
        self.chain.clone()
    }
}

pub struct TestChainSubstreams {
    pub chain: Arc<graph_chain_substreams::Chain>,
    pub block_stream_builder: Arc<graph_chain_substreams::BlockStreamBuilder>,
}

impl TestChainTrait<graph_chain_substreams::Chain> for TestChainSubstreams {
    fn set_block_stream(&self, _blocks: Vec<BlockWithTriggers<graph_chain_substreams::Chain>>) {}

    fn chain(&self) -> Arc<graph_chain_substreams::Chain> {
        self.chain.clone()
    }
}

pub trait TestChainTrait<C: Blockchain> {
    fn set_block_stream(&self, blocks: Vec<BlockWithTriggers<C>>);

    fn chain(&self) -> Arc<C>;
}

pub struct TestContext {
    pub logger: Logger,
    pub provider: Arc<
        IpfsSubgraphAssignmentProvider<
            SubgraphInstanceManager<graph_store_postgres::SubgraphStore>,
        >,
    >,
    pub store: Arc<SubgraphStore>,
    pub deployment: DeploymentLocator,
    pub subgraph_name: SubgraphName,
    pub instance_manager: SubgraphInstanceManager<graph_store_postgres::SubgraphStore>,
    pub link_resolver: Arc<dyn graph::components::link_resolver::LinkResolver>,
    pub arweave_resolver: Arc<dyn ArweaveResolver>,
    pub env_vars: Arc<EnvVars>,
    pub ipfs: IpfsClient,
    graphql_runner: Arc<GraphQlRunner>,
    indexing_status_service: Arc<IndexNodeService<GraphQlRunner, graph_store_postgres::Store>>,
}

#[derive(Deserialize)]
pub struct IndexingStatusBlock {
    pub number: BigInt,
}

#[derive(Deserialize)]
pub struct IndexingStatusError {
    pub deterministic: bool,
    pub block: IndexingStatusBlock,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingStatus {
    pub health: SubgraphHealth,
    pub entity_count: BigInt,
    pub fatal_error: Option<IndexingStatusError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingStatusForCurrentVersion {
    pub indexing_status_for_current_version: IndexingStatus,
}

impl TestContext {
    pub async fn runner(
        &self,
        stop_block: BlockPtr,
    ) -> graph_core::SubgraphRunner<
        graph_chain_ethereum::Chain,
        RuntimeHostBuilder<graph_chain_ethereum::Chain>,
    > {
        let (logger, deployment, raw) = self.get_runner_context().await;
        let tp: Box<dyn TriggerProcessor<_, _>> = Box::new(SubgraphTriggerProcessor {});

        self.instance_manager
            .build_subgraph_runner(
                logger,
                self.env_vars.cheap_clone(),
                deployment,
                raw,
                Some(stop_block.block_number()),
                tp,
            )
            .await
            .unwrap()
    }

    pub async fn runner_substreams(
        &self,
        stop_block: BlockPtr,
    ) -> graph_core::SubgraphRunner<
        graph_chain_substreams::Chain,
        RuntimeHostBuilder<graph_chain_substreams::Chain>,
    > {
        let (logger, deployment, raw) = self.get_runner_context().await;
        let tp: Box<dyn TriggerProcessor<_, _>> = Box::new(SubgraphTriggerProcessor {});

        self.instance_manager
            .build_subgraph_runner(
                logger,
                self.env_vars.cheap_clone(),
                deployment,
                raw,
                Some(stop_block.block_number()),
                tp,
            )
            .await
            .unwrap()
    }

    pub async fn get_runner_context(&self) -> (Logger, DeploymentLocator, serde_yaml::Mapping) {
        let logger = self.logger.cheap_clone();
        let deployment = self.deployment.cheap_clone();

        // Stolen from the IPFS provider, there's prolly a nicer way to re-use it
        let file_bytes = self
            .link_resolver
            .cat(&logger, &deployment.hash.to_ipfs_link())
            .await
            .unwrap();

        let raw: serde_yaml::Mapping = serde_yaml::from_slice(&file_bytes).unwrap();

        (logger, deployment, raw)
    }

    pub async fn start_and_sync_to(&self, stop_block: BlockPtr) {
        // In case the subgraph has been previously started.
        self.provider.stop(self.deployment.clone()).await.unwrap();

        self.provider
            .start(self.deployment.clone(), Some(stop_block.number))
            .await
            .expect("unable to start subgraph");

        wait_for_sync(
            &self.logger,
            self.store.clone(),
            &self.deployment.clone(),
            stop_block,
        )
        .await
        .unwrap();
    }

    pub async fn start_and_sync_to_error(&self, stop_block: BlockPtr) -> SubgraphError {
        // In case the subgraph has been previously started.
        self.provider.stop(self.deployment.clone()).await.unwrap();

        self.provider
            .start(self.deployment.clone(), None)
            .await
            .expect("unable to start subgraph");

        wait_for_sync(
            &self.logger,
            self.store.clone(),
            &self.deployment.clone(),
            stop_block,
        )
        .await
        .unwrap_err()
    }

    pub async fn query(&self, query: &str) -> Result<Option<r::Value>, Vec<QueryError>> {
        let target = QueryTarget::Deployment(self.deployment.hash.clone(), ApiVersion::default());
        let query = Query::new(
            graphql_parser::parse_query(query).unwrap().into_static(),
            None,
            false,
        );
        let query_res = self.graphql_runner.clone().run_query(query, target).await;
        query_res.first().unwrap().duplicate().to_result()
    }

    pub async fn indexing_status(&self) -> IndexingStatus {
        let query = format!(
            r#"
            {{
                indexingStatusForCurrentVersion(subgraphName: "{}") {{
                    health
                    entityCount
                    fatalError {{
                        deterministic
                        block {{
                            number
                        }}
                    }}
                }}
            }}
            "#,
            &self.subgraph_name
        );
        let body = json!({ "query": query }).to_string();
        let req = hyper::Request::new(body.into());
        let res = self.indexing_status_service.handle_graphql_query(req).await;
        let value = res
            .unwrap()
            .first()
            .unwrap()
            .duplicate()
            .to_result()
            .unwrap()
            .unwrap();
        let query_res: IndexingStatusForCurrentVersion =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        query_res.indexing_status_for_current_version
    }

    pub fn rewind(&self, block_ptr_to: BlockPtr) {
        self.store
            .rewind(self.deployment.hash.clone(), block_ptr_to)
            .unwrap()
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        if let Err(e) = cleanup(&self.store, &self.subgraph_name, &self.deployment.hash) {
            crit!(self.logger, "error cleaning up test subgraph"; "error" => e.to_string());
        }
    }
}

pub struct Stores {
    network_name: String,
    chain_head_listener: Arc<ChainHeadUpdateListener>,
    pub network_store: Arc<Store>,
    chain_store: Arc<ChainStore>,
}

graph::prelude::lazy_static! {
    /// Mutex for assuring there's only one test at a time
    /// running the `stores` function.
    pub static ref STORE_MUTEX: Mutex<()> = Mutex::new(());
}

fn test_logger(test_name: &str) -> Logger {
    graph::log::logger(true).new(o!("test" => test_name.to_string()))
}

pub async fn stores(test_name: &str, store_config_path: &str) -> Stores {
    let _mutex_guard = STORE_MUTEX.lock().unwrap();

    let config = {
        let config = read_to_string(store_config_path).await.unwrap();
        let db_url = match std::env::var("THEGRAPH_STORE_POSTGRES_DIESEL_URL") {
            Ok(url) => url,
            Err(VarError::NotPresent) => panic!(
                "to run end-to-end tests it is required to set \
                                            $THEGRAPH_STORE_POSTGRES_DIESEL_URL to the test db url"
            ),
            Err(e) => panic!("{}", e.to_string()),
        };
        let config = config.replace("$THEGRAPH_STORE_POSTGRES_DIESEL_URL", &db_url);
        Config::from_str(&config, "default").expect("failed to create configuration")
    };

    let logger = test_logger(test_name);
    let mock_registry: Arc<MetricsRegistry> = Arc::new(MetricsRegistry::mock());
    let node_id = NodeId::new(NODE_ID).unwrap();
    let store_builder =
        StoreBuilder::new(&logger, &node_id, &config, None, mock_registry.clone()).await;

    let network_name: String = config.chains.chains.iter().next().unwrap().0.to_string();
    let chain_head_listener = store_builder.chain_head_update_listener();
    let network_identifiers = vec![(
        network_name.clone(),
        ChainIdentifier {
            net_version: "".into(),
            genesis_block_hash: test_ptr(0).hash,
        },
    )]
    .into_iter()
    .collect();
    let network_store = store_builder.network_store(network_identifiers);
    let chain_store = network_store
        .block_store()
        .chain_store(network_name.as_ref())
        .unwrap_or_else(|| panic!("No chain store for {}", &network_name));

    Stores {
        network_name,
        chain_head_listener,
        network_store,
        chain_store,
    }
}

pub async fn setup<C: Blockchain>(
    test_name: &str,
    subgraph_name: SubgraphName,
    hash: &DeploymentHash,
    stores: &Stores,
    chain: &impl TestChainTrait<C>,
    graft_block: Option<BlockPtr>,
    env_vars: Option<EnvVars>,
) -> TestContext {
    let env_vars = Arc::new(match env_vars {
        Some(ev) => ev,
        None => EnvVars::from_env().unwrap(),
    });

    let logger = test_logger(test_name);
    let mock_registry: Arc<MetricsRegistry> = Arc::new(MetricsRegistry::mock());
    let logger_factory = LoggerFactory::new(logger.clone(), None, mock_registry.clone());
    let node_id = NodeId::new(NODE_ID).unwrap();

    // Make sure we're starting from a clean state.
    let subgraph_store = stores.network_store.subgraph_store();
    cleanup(&subgraph_store, &subgraph_name, hash).unwrap();

    let mut blockchain_map = BlockchainMap::new();
    blockchain_map.insert(stores.network_name.clone(), chain.chain());

    let static_filters = env_vars.experimental_static_filters;

    let ipfs = IpfsClient::localhost();
    let link_resolver = Arc::new(LinkResolver::new(
        vec![ipfs.cheap_clone()],
        Default::default(),
    ));
    let ipfs_service = ipfs_service(
        ipfs.cheap_clone(),
        env_vars.mappings.max_ipfs_file_bytes as u64,
        env_vars.mappings.ipfs_timeout,
        env_vars.mappings.ipfs_request_limit,
    );

    let arweave_resolver = Arc::new(ArweaveClient::default());
    let arweave_service = arweave_service(
        arweave_resolver.cheap_clone(),
        env_vars.mappings.ipfs_timeout,
        env_vars.mappings.ipfs_request_limit,
        match env_vars.mappings.max_ipfs_file_bytes {
            0 => FileSizeLimit::Unlimited,
            n => FileSizeLimit::MaxBytes(n as u64),
        },
    );
    let sg_count = Arc::new(SubgraphCountMetric::new(mock_registry.cheap_clone()));

    let blockchain_map = Arc::new(blockchain_map);
    let subgraph_instance_manager = SubgraphInstanceManager::new(
        &logger_factory,
        env_vars.cheap_clone(),
        subgraph_store.clone(),
        blockchain_map.clone(),
        sg_count.cheap_clone(),
        mock_registry.clone(),
        link_resolver.cheap_clone(),
        ipfs_service,
        arweave_service,
        static_filters,
    );

    // Graphql runner
    let subscription_manager = Arc::new(PanicSubscriptionManager {});
    let load_manager = LoadManager::new(&logger, Vec::new(), Vec::new(), mock_registry.clone());
    let graphql_runner = Arc::new(GraphQlRunner::new(
        &logger,
        stores.network_store.clone(),
        subscription_manager.clone(),
        Arc::new(load_manager),
        mock_registry.clone(),
    ));

    let indexing_status_service = Arc::new(IndexNodeService::new(
        logger.cheap_clone(),
        blockchain_map.cheap_clone(),
        graphql_runner.cheap_clone(),
        stores.network_store.cheap_clone(),
        link_resolver.cheap_clone(),
    ));

    // Create IPFS-based subgraph provider
    let subgraph_provider = Arc::new(IpfsSubgraphAssignmentProvider::new(
        &logger_factory,
        link_resolver.cheap_clone(),
        subgraph_instance_manager.clone(),
        sg_count,
    ));

    let panicking_subscription_manager = Arc::new(PanicSubscriptionManager {});

    let subgraph_registrar = Arc::new(IpfsSubgraphRegistrar::new(
        &logger_factory,
        link_resolver.cheap_clone(),
        subgraph_provider.clone(),
        subgraph_store.clone(),
        panicking_subscription_manager,
        blockchain_map.clone(),
        node_id.clone(),
        SubgraphVersionSwitchingMode::Instant,
        Arc::new(Settings::default()),
    ));

    SubgraphRegistrar::create_subgraph(subgraph_registrar.as_ref(), subgraph_name.clone())
        .await
        .expect("unable to create subgraph");

    let deployment = SubgraphRegistrar::create_subgraph_version(
        subgraph_registrar.as_ref(),
        subgraph_name.clone(),
        hash.clone(),
        node_id.clone(),
        None,
        None,
        graft_block,
        None,
    )
    .await
    .expect("failed to create subgraph version");

    let arweave_resolver = Arc::new(ArweaveClient::default());

    TestContext {
        logger: logger_factory.subgraph_logger(&deployment),
        provider: subgraph_provider,
        store: subgraph_store,
        deployment,
        subgraph_name,
        graphql_runner,
        instance_manager: subgraph_instance_manager,
        link_resolver,
        env_vars,
        indexing_status_service,
        ipfs,
        arweave_resolver,
    }
}

pub fn cleanup(
    subgraph_store: &SubgraphStore,
    name: &SubgraphName,
    hash: &DeploymentHash,
) -> Result<(), Error> {
    let locators = subgraph_store.locators(hash)?;
    subgraph_store.remove_subgraph(name.clone())?;
    for locator in locators {
        subgraph_store.remove_deployment(locator.id.into())?;
    }
    Ok(())
}

pub async fn wait_for_sync(
    logger: &Logger,
    store: Arc<SubgraphStore>,
    deployment: &DeploymentLocator,
    stop_block: BlockPtr,
) -> Result<(), SubgraphError> {
    // We wait one second between checks for the subgraph to sync. That
    // means we wait up to a minute here by default
    lazy_static! {
        static ref MAX_ERR_COUNT: usize = std::env::var("RUNNER_TESTS_WAIT_FOR_SYNC_SECS")
            .map(|val| val.parse().unwrap())
            .unwrap_or(60);
    }
    const WAIT_TIME: Duration = Duration::from_secs(1);

    /// We flush here to speed up how long the write queue waits before it
    /// considers a batch complete and writable. Without flushing, we would
    /// have to wait for `GRAPH_STORE_WRITE_BATCH_DURATION` before all
    /// changes have been written to the database
    async fn flush(logger: &Logger, store: &Arc<SubgraphStore>, deployment: &DeploymentLocator) {
        store
            .clone()
            .writable(logger.clone(), deployment.id, Arc::new(vec![]))
            .await
            .unwrap()
            .flush()
            .await
            .unwrap();
    }

    let mut err_count = 0;

    flush(logger, &store, deployment).await;

    while err_count < *MAX_ERR_COUNT {
        tokio::time::sleep(WAIT_TIME).await;
        flush(logger, &store, deployment).await;

        let block_ptr = match store.least_block_ptr(&deployment.hash).await {
            Ok(Some(ptr)) => ptr,
            res => {
                info!(&logger, "{:?}", res);
                err_count += 1;
                continue;
            }
        };
        info!(logger, "TEST: sync status: {:?}", block_ptr);
        let status = store.status_for_id(deployment.id);

        if let Some(fatal_error) = status.fatal_error {
            if fatal_error.block_ptr.as_ref().unwrap() == &stop_block {
                return Err(fatal_error);
            }
        }

        if block_ptr == stop_block {
            info!(logger, "TEST: reached stop block");
            return Ok(());
        }
    }

    // We only get here if we timed out waiting for the subgraph to reach
    // the stop block
    crit!(logger, "TEST: sync never completed (err_count={err_count})");
    panic!("Sync did not complete within {err_count}s");
}

struct StaticBlockRefetcher<C: Blockchain> {
    x: PhantomData<C>,
}

#[async_trait]
impl<C: Blockchain> BlockRefetcher<C> for StaticBlockRefetcher<C> {
    fn required(&self, _chain: &C) -> bool {
        false
    }

    async fn get_block(
        &self,
        _chain: &C,
        _logger: &Logger,
        _cursor: FirehoseCursor,
    ) -> Result<C::Block, Error> {
        unimplemented!("this block refetcher always returns false, get_block shouldn't be called")
    }
}

pub struct MutexBlockStreamBuilder<C: Blockchain>(pub Mutex<Arc<dyn BlockStreamBuilder<C>>>);

#[async_trait]
impl<C: Blockchain> BlockStreamBuilder<C> for MutexBlockStreamBuilder<C> {
    async fn build_firehose(
        &self,
        chain: &C,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        start_blocks: Vec<BlockNumber>,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<<C as Blockchain>::TriggerFilter>,
        unified_api_version: graph::data::subgraph::UnifiedMappingApiVersion,
    ) -> anyhow::Result<Box<dyn BlockStream<C>>> {
        let builder = self.0.lock().unwrap().clone();

        builder
            .build_firehose(
                chain,
                deployment,
                block_cursor,
                start_blocks,
                subgraph_current_block,
                filter,
                unified_api_version,
            )
            .await
    }

    async fn build_substreams(
        &self,
        _chain: &C,
        _schema: InputSchema,
        _deployment: DeploymentLocator,
        _block_cursor: FirehoseCursor,
        _subgraph_current_block: Option<BlockPtr>,
        _filter: Arc<C::TriggerFilter>,
    ) -> anyhow::Result<Box<dyn BlockStream<C>>> {
        unimplemented!();
    }

    async fn build_polling(
        &self,
        _chain: &C,
        _deployment: DeploymentLocator,
        _start_blocks: Vec<BlockNumber>,
        _subgraph_current_block: Option<BlockPtr>,
        _filter: Arc<<C as Blockchain>::TriggerFilter>,
        _unified_api_version: graph::data::subgraph::UnifiedMappingApiVersion,
    ) -> anyhow::Result<Box<dyn BlockStream<C>>> {
        unimplemented!("only firehose mode should be used for tests")
    }
}

/// `chain` is the sequence of chain heads to be processed. If the next block to be processed in the
/// chain is not a descendant of the previous one, reorgs will be emitted until it is.
///
/// If the stream is reset, emitted reorged blocks will not be emitted again.
/// See also: static-stream-builder
struct StaticStreamBuilder<C: Blockchain> {
    chain: Vec<BlockWithTriggers<C>>,
}

#[async_trait]
impl<C: Blockchain> BlockStreamBuilder<C> for StaticStreamBuilder<C>
where
    C::TriggerData: Clone,
{
    async fn build_substreams(
        &self,
        _chain: &C,
        _schema: InputSchema,
        _deployment: DeploymentLocator,
        _block_cursor: FirehoseCursor,
        _subgraph_current_block: Option<BlockPtr>,
        _filter: Arc<C::TriggerFilter>,
    ) -> anyhow::Result<Box<dyn BlockStream<C>>> {
        unimplemented!()
    }

    async fn build_firehose(
        &self,
        _chain: &C,
        _deployment: DeploymentLocator,
        _block_cursor: FirehoseCursor,
        _start_blocks: Vec<graph::prelude::BlockNumber>,
        current_block: Option<graph::blockchain::BlockPtr>,
        _filter: Arc<C::TriggerFilter>,
        _unified_api_version: graph::data::subgraph::UnifiedMappingApiVersion,
    ) -> anyhow::Result<Box<dyn BlockStream<C>>> {
        let current_idx = current_block.map(|current_block| {
            self.chain
                .iter()
                .enumerate()
                .find(|(_, b)| b.ptr() == current_block)
                .unwrap()
                .0
        });
        Ok(Box::new(StaticStream {
            stream: Box::pin(stream_events(self.chain.clone(), current_idx)),
        }))
    }

    async fn build_polling(
        &self,
        _chain: &C,
        _deployment: DeploymentLocator,
        _start_blocks: Vec<graph::prelude::BlockNumber>,
        _subgraph_current_block: Option<graph::blockchain::BlockPtr>,
        _filter: Arc<C::TriggerFilter>,
        _unified_api_version: graph::data::subgraph::UnifiedMappingApiVersion,
    ) -> anyhow::Result<Box<dyn BlockStream<C>>> {
        unimplemented!("only firehose mode should be used for tests")
    }
}

struct StaticStream<C: Blockchain> {
    stream: Pin<Box<dyn Stream<Item = Result<BlockStreamEvent<C>, Error>> + Send>>,
}

impl<C: Blockchain> BlockStream<C> for StaticStream<C> {
    fn buffer_size_hint(&self) -> usize {
        1
    }
}

impl<C: Blockchain> Stream for StaticStream<C> {
    type Item = Result<BlockStreamEvent<C>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

fn stream_events<C: Blockchain>(
    blocks: Vec<BlockWithTriggers<C>>,
    current_idx: Option<usize>,
) -> impl Stream<Item = Result<BlockStreamEvent<C>, Error>>
where
    C::TriggerData: Clone,
{
    // See also: static-stream-builder
    stream! {
        let current_block = current_idx.map(|idx| &blocks[idx]);
        let mut current_ptr = current_block.map(|b| b.ptr());
        let mut current_parent_ptr = current_block.and_then(|b| b.parent_ptr());
        let skip = current_idx.map(|idx| idx + 1).unwrap_or(0);
        let mut blocks_iter = blocks.iter().skip(skip).peekable();
        while let Some(&block) = blocks_iter.peek() {
            if block.parent_ptr() == current_ptr {
                current_ptr = Some(block.ptr());
                current_parent_ptr = block.parent_ptr();
                blocks_iter.next(); // Block consumed, advance the iterator.
                yield Ok(BlockStreamEvent::ProcessBlock(block.clone(), FirehoseCursor::None));
            } else {
                let revert_to = current_parent_ptr.unwrap();
                current_ptr = Some(revert_to.clone());
                current_parent_ptr = blocks
                    .iter()
                    .find(|b| b.ptr() == revert_to)
                    .unwrap()
                    .block
                    .parent_ptr();
                yield Ok(BlockStreamEvent::Revert(revert_to, FirehoseCursor::None));
            }
        }
    }
}

struct NoopRuntimeAdapter<C> {
    x: PhantomData<C>,
}

impl<C: Blockchain> RuntimeAdapter<C> for NoopRuntimeAdapter<C> {
    fn host_fns(
        &self,
        _ds: &<C as Blockchain>::DataSource,
    ) -> Result<Vec<graph::blockchain::HostFn>, Error> {
        Ok(vec![])
    }
}

pub struct NoopAdapterSelector<C> {
    pub x: PhantomData<C>,
    pub triggers_in_block_sleep: Duration,
}

impl<C: Blockchain> TriggersAdapterSelector<C> for NoopAdapterSelector<C> {
    fn triggers_adapter(
        &self,
        _loc: &DeploymentLocator,
        _capabilities: &<C as Blockchain>::NodeCapabilities,
        _unified_api_version: graph::data::subgraph::UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn graph::blockchain::TriggersAdapter<C>>, Error> {
        // Return no triggers on data source reprocessing.
        let triggers_in_block = Arc::new(|block| {
            let logger = Logger::root(Discard, o!());
            Ok(BlockWithTriggers::new(block, Vec::new(), &logger))
        });
        Ok(Arc::new(MockTriggersAdapter {
            x: PhantomData,
            triggers_in_block,
            triggers_in_block_sleep: self.triggers_in_block_sleep,
        }))
    }
}

pub struct MockAdapterSelector<C: Blockchain> {
    pub x: PhantomData<C>,
    pub triggers_in_block_sleep: Duration,
    pub triggers_in_block:
        Arc<dyn Fn(<C as Blockchain>::Block) -> Result<BlockWithTriggers<C>, Error> + Sync + Send>,
}

impl<C: Blockchain> TriggersAdapterSelector<C> for MockAdapterSelector<C> {
    fn triggers_adapter(
        &self,
        _loc: &DeploymentLocator,
        _capabilities: &<C as Blockchain>::NodeCapabilities,
        _unified_api_version: graph::data::subgraph::UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn graph::blockchain::TriggersAdapter<C>>, Error> {
        Ok(Arc::new(MockTriggersAdapter {
            x: PhantomData,
            triggers_in_block: self.triggers_in_block.clone(),
            triggers_in_block_sleep: self.triggers_in_block_sleep,
        }))
    }
}

struct MockTriggersAdapter<C: Blockchain> {
    x: PhantomData<C>,
    triggers_in_block_sleep: Duration,
    triggers_in_block:
        Arc<dyn Fn(<C as Blockchain>::Block) -> Result<BlockWithTriggers<C>, Error> + Sync + Send>,
}

#[async_trait]
impl<C: Blockchain> TriggersAdapter<C> for MockTriggersAdapter<C> {
    async fn ancestor_block(
        &self,
        _ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<<C as Blockchain>::Block>, Error> {
        todo!()
    }

    async fn scan_triggers(
        &self,
        _from: BlockNumber,
        _to: BlockNumber,
        _filter: &<C as Blockchain>::TriggerFilter,
    ) -> Result<Vec<BlockWithTriggers<C>>, Error> {
        todo!()
    }

    async fn triggers_in_block(
        &self,
        _logger: &Logger,
        block: <C as Blockchain>::Block,
        _filter: &<C as Blockchain>::TriggerFilter,
    ) -> Result<BlockWithTriggers<C>, Error> {
        tokio::time::sleep(self.triggers_in_block_sleep).await;

        (self.triggers_in_block)(block)
    }

    async fn is_on_main_chain(&self, _ptr: BlockPtr) -> Result<bool, Error> {
        todo!()
    }

    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        match block.number {
            0 => Ok(None),
            n => Ok(Some(BlockPtr {
                hash: BlockHash::default(),
                number: n - 1,
            })),
        }
    }
}
