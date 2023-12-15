use diesel::{self, PgConnection};
use graph::blockchain::mock::MockDataSource;
use graph::data::graphql::load_manager::LoadManager;
use graph::data::query::QueryResults;
use graph::data::query::QueryTarget;
use graph::data::subgraph::schema::{DeploymentCreate, SubgraphError};
use graph::data::subgraph::SubgraphFeature;
use graph::data_source::DataSource;
use graph::log;
use graph::prelude::{QueryStoreManager as _, SubgraphStore as _, *};
use graph::schema::EntityType;
use graph::schema::InputSchema;
use graph::semver::Version;
use graph::{
    blockchain::block_stream::FirehoseCursor, blockchain::ChainIdentifier,
    components::store::DeploymentLocator, components::store::StatusStore,
    components::store::StoredDynamicDataSource, data::subgraph::status, prelude::NodeId,
};
use graph_graphql::prelude::{
    execute_query, Query as PreparedQuery, QueryExecutionOptions, StoreResolver,
};
use graph_graphql::test_support::GraphQLMetrics;
use graph_node::config::{Config, Opt};
use graph_node::store_builder::StoreBuilder;
use graph_store_postgres::layout_for_tests::FAKE_NETWORK_SHARED;
use graph_store_postgres::{connection_pool::ConnectionPool, Shard, SubscriptionManager};
use graph_store_postgres::{
    BlockStore as DieselBlockStore, DeploymentPlacer, SubgraphStore as DieselSubgraphStore,
    PRIMARY_SHARD,
};
use hex_literal::hex;
use lazy_static::lazy_static;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::time::Instant;
use std::{marker::PhantomData, sync::Mutex};
use tokio::runtime::{Builder, Runtime};
use web3::types::H256;

pub const NETWORK_NAME: &str = "fake_network";
pub const DATA_SOURCE_KIND: &str = "mock/kind";
pub const NETWORK_VERSION: &str = "graph test suite";

pub use graph_store_postgres::Store;

const CONN_POOL_SIZE: u32 = 20;

lazy_static! {
    pub static ref LOGGER: Logger = match ENV_VARS.log_levels {
        Some(_) => log::logger(false),
        None => Logger::root(slog::Discard, o!()),
    };
    static ref SEQ_LOCK: Mutex<()> = Mutex::new(());
    pub static ref STORE_RUNTIME: Runtime =
        Builder::new_multi_thread().enable_all().build().unwrap();
    pub static ref METRICS_REGISTRY: Arc<MetricsRegistry> = Arc::new(MetricsRegistry::mock());
    pub static ref LOAD_MANAGER: Arc<LoadManager> = Arc::new(LoadManager::new(
        &LOGGER,
        CONFIG.stores.keys().cloned().collect(),
        Vec::new(),
        METRICS_REGISTRY.clone(),
    ));
    static ref STORE_POOL_CONFIG: (Arc<Store>, ConnectionPool, Config, Arc<SubscriptionManager>) =
        build_store();
    pub(crate) static ref PRIMARY_POOL: ConnectionPool = STORE_POOL_CONFIG.1.clone();
    pub static ref STORE: Arc<Store> = STORE_POOL_CONFIG.0.clone();
    static ref CONFIG: Config = STORE_POOL_CONFIG.2.clone();
    pub static ref SUBSCRIPTION_MANAGER: Arc<SubscriptionManager> = STORE_POOL_CONFIG.3.clone();
    pub static ref NODE_ID: NodeId = NodeId::new("test").unwrap();
    pub static ref SUBGRAPH_STORE: Arc<DieselSubgraphStore> = STORE.subgraph_store();
    static ref BLOCK_STORE: Arc<DieselBlockStore> = STORE.block_store();
    pub static ref GENESIS_PTR: BlockPtr = (
        H256::from(hex!(
            "bd34884280958002c51d3f7b5f853e6febeba33de0f40d15b0363006533c924f"
        )),
        0u64
    )
        .into();
    pub static ref BLOCK_ONE: BlockPtr = (
        H256::from(hex!(
            "8511fa04b64657581e3f00e14543c1d522d5d7e771b54aa3060b662ade47da13"
        )),
        1u64
    )
        .into();
    pub static ref BLOCKS: [BlockPtr; 4] = {
        let two: BlockPtr = (
            H256::from(hex!(
                "b98fb783b49de5652097a989414c767824dff7e7fd765a63b493772511db81c1"
            )),
            2u64,
        )
            .into();
        let three: BlockPtr = (
            H256::from(hex!(
                "977c084229c72a0fa377cae304eda9099b6a2cb5d83b25cdf0f0969b69874255"
            )),
            3u64,
        )
            .into();
        [GENESIS_PTR.clone(), BLOCK_ONE.clone(), two, three]
    };
}

/// Run the `test` after performing `setup`. The result of `setup` is passed
/// into `test`. All tests using `run_test_sequentially` are run in sequence,
/// never in parallel. The `test` is passed a `Store`, but it is permissible
/// for tests to access the global `STORE` from this module, too.
pub fn run_test_sequentially<R, F>(test: F)
where
    F: FnOnce(Arc<Store>) -> R + Send + 'static,
    R: std::future::Future<Output = ()> + Send + 'static,
{
    // Lock regardless of poisoning. This also forces sequential test execution.
    let _lock = match SEQ_LOCK.lock() {
        Ok(guard) => guard,
        Err(err) => err.into_inner(),
    };

    STORE_RUNTIME.handle().block_on(async {
        let store = STORE.clone();
        test(store).await
    })
}

/// Run a test with a connection into the primary database, not a full store
pub fn run_test_with_conn<F>(test: F)
where
    F: FnOnce(&PgConnection),
{
    // Lock regardless of poisoning. This also forces sequential test execution.
    let _lock = match SEQ_LOCK.lock() {
        Ok(guard) => guard,
        Err(err) => err.into_inner(),
    };

    let conn = PRIMARY_POOL
        .get()
        .expect("failed to get connection for primary database");

    test(&conn);
}

pub fn remove_subgraphs() {
    SUBGRAPH_STORE
        .delete_all_entities_for_test_use_only()
        .expect("deleting test entities succeeds");
}

pub fn place(name: &str) -> Result<Option<(Vec<Shard>, Vec<NodeId>)>, String> {
    CONFIG.deployment.place(name, NETWORK_NAME)
}

pub async fn create_subgraph(
    subgraph_id: &DeploymentHash,
    schema: &str,
    base: Option<(DeploymentHash, BlockPtr)>,
) -> Result<DeploymentLocator, StoreError> {
    let schema = InputSchema::parse(schema, subgraph_id.clone()).unwrap();

    let manifest = SubgraphManifest::<graph::blockchain::mock::MockBlockchain> {
        id: subgraph_id.clone(),
        spec_version: Version::new(1, 0, 0),
        features: BTreeSet::new(),
        description: Some(format!("manifest for {}", subgraph_id)),
        repository: Some(format!("repo for {}", subgraph_id)),
        schema: schema.clone(),
        data_sources: vec![],
        graft: None,
        templates: vec![],
        chain: PhantomData,
        indexer_hints: None,
    };

    create_subgraph_with_manifest(subgraph_id, schema, manifest, base).await
}

pub async fn create_subgraph_with_manifest(
    subgraph_id: &DeploymentHash,
    schema: InputSchema,
    manifest: SubgraphManifest<graph::blockchain::mock::MockBlockchain>,
    base: Option<(DeploymentHash, BlockPtr)>,
) -> Result<DeploymentLocator, StoreError> {
    let mut yaml = serde_yaml::Mapping::new();
    yaml.insert("dataSources".into(), Vec::<serde_yaml::Value>::new().into());
    let yaml = serde_yaml::to_string(&yaml).unwrap();
    let deployment = DeploymentCreate::new(yaml, &manifest, None).graft(base);
    let name = SubgraphName::new_unchecked(subgraph_id.to_string());
    let deployment = SUBGRAPH_STORE.create_deployment_replace(
        name,
        &schema,
        deployment,
        NODE_ID.clone(),
        NETWORK_NAME.to_string(),
        SubgraphVersionSwitchingMode::Instant,
    )?;

    SUBGRAPH_STORE
        .cheap_clone()
        .writable(LOGGER.clone(), deployment.id, Arc::new(Vec::new()))
        .await?
        .start_subgraph_deployment(&LOGGER)
        .await?;
    Ok(deployment)
}

pub async fn create_test_subgraph(subgraph_id: &DeploymentHash, schema: &str) -> DeploymentLocator {
    create_subgraph(subgraph_id, schema, None).await.unwrap()
}

pub async fn create_test_subgraph_with_features(
    subgraph_id: &DeploymentHash,
    schema: &str,
) -> DeploymentLocator {
    let schema = InputSchema::parse(schema, subgraph_id.clone()).unwrap();

    let features = [
        SubgraphFeature::FullTextSearch,
        SubgraphFeature::NonFatalErrors,
    ]
    .iter()
    .cloned()
    .collect::<BTreeSet<_>>();

    let manifest = SubgraphManifest::<graph::blockchain::mock::MockBlockchain> {
        id: subgraph_id.clone(),
        spec_version: Version::new(1, 0, 0),
        features: features,
        description: Some(format!("manifest for {}", subgraph_id)),
        repository: Some(format!("repo for {}", subgraph_id)),
        schema: schema.clone(),
        data_sources: vec![DataSource::Onchain(MockDataSource {
            kind: DATA_SOURCE_KIND.into(),
            api_version: Version::new(1, 0, 0),
            network: Some(NETWORK_NAME.into()),
        })],
        graft: None,
        templates: vec![],
        chain: PhantomData,
        indexer_hints: None,
    };

    let deployment_features = manifest.deployment_features();

    let locator = create_subgraph_with_manifest(subgraph_id, schema, manifest, None)
        .await
        .unwrap();

    SUBGRAPH_STORE
        .create_subgraph_features(deployment_features)
        .unwrap();

    locator
}

pub fn remove_subgraph(id: &DeploymentHash) {
    let name = SubgraphName::new_unchecked(id.to_string());
    SUBGRAPH_STORE.remove_subgraph(name).unwrap();
    let locs = SUBGRAPH_STORE.locators(id.as_str()).unwrap();
    let conn = primary_connection();
    for loc in locs {
        let site = conn.locate_site(loc.clone()).unwrap().unwrap();
        conn.unassign_subgraph(&site).unwrap();
        SUBGRAPH_STORE.remove_deployment(site.id).unwrap();
    }
}

/// Transact errors for this block and wait until changes have been written
/// Takes store, deployment, block ptr to, errors, and a bool indicating whether
/// nonFatalErrors are active
pub async fn transact_errors(
    store: &Arc<Store>,
    deployment: &DeploymentLocator,
    block_ptr_to: BlockPtr,
    errs: Vec<SubgraphError>,
    is_non_fatal_errors_active: bool,
) -> Result<(), StoreError> {
    let metrics_registry = Arc::new(MetricsRegistry::mock());
    let stopwatch_metrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, o!()),
        deployment.hash.clone(),
        "transact",
        metrics_registry.clone(),
        store.subgraph_store().shard(deployment)?.to_string(),
    );
    store
        .subgraph_store()
        .writable(LOGGER.clone(), deployment.id, Arc::new(Vec::new()))
        .await?
        .transact_block_operations(
            block_ptr_to,
            FirehoseCursor::None,
            Vec::new(),
            &stopwatch_metrics,
            Vec::new(),
            errs,
            Vec::new(),
            is_non_fatal_errors_active,
        )
        .await?;
    flush(deployment).await
}

/// Convenience to transact EntityOperation instead of EntityModification
pub async fn transact_entity_operations(
    store: &Arc<DieselSubgraphStore>,
    deployment: &DeploymentLocator,
    block_ptr_to: BlockPtr,
    ops: Vec<EntityOperation>,
) -> Result<(), StoreError> {
    transact_entities_and_dynamic_data_sources(
        store,
        deployment.clone(),
        block_ptr_to,
        vec![],
        ops,
        vec![],
    )
    .await
}

/// Convenience to transact EntityOperation instead of EntityModification and wait for the store to process the operations
pub async fn transact_and_wait(
    store: &Arc<DieselSubgraphStore>,
    deployment: &DeploymentLocator,
    block_ptr_to: BlockPtr,
    ops: Vec<EntityOperation>,
) -> Result<(), StoreError> {
    transact_entities_and_dynamic_data_sources(
        store,
        deployment.clone(),
        block_ptr_to,
        vec![],
        ops,
        vec![],
    )
    .await?;
    flush(deployment).await
}

pub async fn transact_entities_and_dynamic_data_sources(
    store: &Arc<DieselSubgraphStore>,
    deployment: DeploymentLocator,
    block_ptr_to: BlockPtr,
    data_sources: Vec<StoredDynamicDataSource>,
    ops: Vec<EntityOperation>,
    manifest_idx_and_name: Vec<(u32, String)>,
) -> Result<(), StoreError> {
    let store = futures03::executor::block_on(store.cheap_clone().writable(
        LOGGER.clone(),
        deployment.id,
        Arc::new(manifest_idx_and_name),
    ))?;

    let mut entity_cache = EntityCache::new(Arc::new(store.clone()));
    entity_cache.append(ops);
    let mods = entity_cache
        .as_modifications(block_ptr_to.number)
        .expect("failed to convert to modifications")
        .modifications;
    let metrics_registry = Arc::new(MetricsRegistry::mock());
    let stopwatch_metrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, o!()),
        deployment.hash.clone(),
        "transact",
        metrics_registry.clone(),
        store.shard().to_string(),
    );
    store
        .transact_block_operations(
            block_ptr_to,
            FirehoseCursor::None,
            mods,
            &stopwatch_metrics,
            data_sources,
            Vec::new(),
            Vec::new(),
            false,
        )
        .await
}

/// Revert to block `ptr` and wait for the store to process the changes
pub async fn revert_block(store: &Arc<Store>, deployment: &DeploymentLocator, ptr: &BlockPtr) {
    store
        .subgraph_store()
        .writable(LOGGER.clone(), deployment.id, Arc::new(Vec::new()))
        .await
        .expect("can get writable")
        .revert_block_operations(ptr.clone(), FirehoseCursor::None)
        .await
        .unwrap();
    flush(deployment).await.unwrap();
}

pub fn insert_ens_name(hash: &str, name: &str) {
    use diesel::insert_into;
    use diesel::prelude::*;
    use graph_store_postgres::command_support::catalog::ens_names;

    let conn = PRIMARY_POOL.get().unwrap();

    insert_into(ens_names::table)
        .values((ens_names::hash.eq(hash), ens_names::name.eq(name)))
        .on_conflict_do_nothing()
        .execute(&conn)
        .unwrap();
}

/// Insert the given entities and wait until all writes have been processed.
/// The inserts all happen at `GENESIS_PTR`, i.e., block 0
pub async fn insert_entities(
    deployment: &DeploymentLocator,
    entities: Vec<(EntityType, Entity)>,
) -> Result<(), StoreError> {
    let insert_ops = entities
        .into_iter()
        .map(|(entity_type, data)| EntityOperation::Set {
            key: entity_type.key(data.id()),
            data,
        });

    transact_entity_operations(
        &SUBGRAPH_STORE,
        deployment,
        GENESIS_PTR.clone(),
        insert_ops.collect::<Vec<_>>(),
    )
    .await?;

    flush(deployment).await
}

/// Wait until all pending writes have been processed
pub async fn flush(deployment: &DeploymentLocator) -> Result<(), StoreError> {
    let writable = SUBGRAPH_STORE
        .cheap_clone()
        .writable(LOGGER.clone(), deployment.id, Arc::new(Vec::new()))
        .await
        .expect("we can get a writable");
    writable.flush().await
}

/// Tap into store events sent when running `f` and return those events. This
/// intercepts `StoreEvent` when they are sent and therefore does not require
/// the delicate timing that actually listening to events in the database
/// requires. Of course, this does not test that events that are sent are
/// actually received by anything, but makes ensuring that the right events
/// get sent much more convenient than trying to receive them
pub fn tap_store_events<F, R>(f: F) -> (R, Vec<StoreEvent>)
where
    F: FnOnce() -> R,
{
    use graph_store_postgres::layout_for_tests::{EVENT_TAP, EVENT_TAP_ENABLED};

    EVENT_TAP.lock().unwrap().clear();
    *EVENT_TAP_ENABLED.lock().unwrap() = true;
    let res = f();
    *EVENT_TAP_ENABLED.lock().unwrap() = false;
    (res, EVENT_TAP.lock().unwrap().clone())
}

/// Run a GraphQL query against the `STORE`
pub async fn execute_subgraph_query(query: Query, target: QueryTarget) -> QueryResults {
    execute_subgraph_query_internal(query, target, None, None).await
}

pub async fn execute_subgraph_query_with_deadline(
    query: Query,
    target: QueryTarget,
    deadline: Option<Instant>,
) -> QueryResults {
    execute_subgraph_query_internal(query, target, None, deadline).await
}

/// Like `try!`, but we return the contents of an `Err`, not the
/// whole `Result`
#[macro_export]
macro_rules! return_err {
    ( $ expr : expr ) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return e.into(),
        }
    };
}

pub fn graphql_metrics() -> Arc<GraphQLMetrics> {
    Arc::new(GraphQLMetrics::make(METRICS_REGISTRY.clone()))
}

async fn execute_subgraph_query_internal(
    query: Query,
    target: QueryTarget,
    max_complexity: Option<u64>,
    deadline: Option<Instant>,
) -> QueryResults {
    let logger = Logger::root(slog::Discard, o!());
    let (id, version) = match target {
        QueryTarget::Deployment(id, version) => (id, version),
        _ => unreachable!("tests do not use this"),
    };
    let schema = SUBGRAPH_STORE.api_schema(&id, &Default::default()).unwrap();
    let status = StatusStore::status(
        STORE.as_ref(),
        status::Filter::Deployments(vec![id.to_string()]),
    )
    .unwrap();
    let network = Some(status[0].chains[0].network.clone());
    let trace = query.trace;
    let query = return_err!(PreparedQuery::new(
        &logger,
        schema,
        network,
        query,
        max_complexity,
        100,
        graphql_metrics(),
    ));
    let mut result = QueryResults::empty();
    let deployment = query.schema.id().clone();
    let store = STORE
        .clone()
        .query_store(QueryTarget::Deployment(deployment, version.clone()), false)
        .await
        .unwrap();
    let state = store.deployment_state().await.unwrap();
    for (bc, (selection_set, error_policy)) in return_err!(query.block_constraint()) {
        let logger = logger.clone();
        let resolver = return_err!(
            StoreResolver::at_block(
                &logger,
                store.clone(),
                &state,
                SUBSCRIPTION_MANAGER.clone(),
                bc,
                error_policy,
                query.schema.id().clone(),
                graphql_metrics(),
                LOAD_MANAGER.clone()
            )
            .await
        );
        result.append(
            execute_query(
                query.clone(),
                Some(selection_set),
                None,
                QueryExecutionOptions {
                    resolver,
                    deadline,
                    max_first: std::u32::MAX,
                    max_skip: std::u32::MAX,
                    trace,
                },
            )
            .await,
        )
    }
    result
}

pub async fn deployment_state(store: &Store, subgraph_id: &DeploymentHash) -> DeploymentState {
    store
        .query_store(
            QueryTarget::Deployment(subgraph_id.clone(), Default::default()),
            false,
        )
        .await
        .expect("could get a query store")
        .deployment_state()
        .await
        .expect("can get deployment state")
}

pub fn store_is_sharded() -> bool {
    CONFIG.stores.len() > 1
}

pub fn all_shards() -> Vec<Shard> {
    CONFIG
        .stores
        .keys()
        .map(|shard| Shard::new(shard.clone()))
        .collect::<Result<Vec<_>, _>>()
        .expect("all configured shard names are valid")
}

fn build_store() -> (Arc<Store>, ConnectionPool, Config, Arc<SubscriptionManager>) {
    let mut opt = Opt::default();
    let url = std::env::var_os("THEGRAPH_STORE_POSTGRES_DIESEL_URL").filter(|s| s.len() > 0);
    let file = std::env::var_os("GRAPH_NODE_TEST_CONFIG").filter(|s| s.len() > 0);
    if let Some(file) = file {
        let file = file.into_string().unwrap();
        opt.config = Some(file);
        if url.is_some() {
            eprintln!("WARNING: ignoring THEGRAPH_STORE_POSTGRES_DIESEL_URL because GRAPH_NODE_TEST_CONFIG is set");
        }
    } else if let Some(url) = url {
        let url = url.into_string().unwrap();
        opt.postgres_url = Some(url);
    } else {
        panic!("You must set either THEGRAPH_STORE_POSTGRES_DIESEL_URL or GRAPH_NODE_TEST_CONFIG (see ./CONTRIBUTING.md).");
    }
    opt.store_connection_pool_size = CONN_POOL_SIZE;

    let config = Config::load(&LOGGER, &opt)
        .unwrap_or_else(|_| panic!("config is not valid (file={:?})", &opt.config));
    let registry = Arc::new(MetricsRegistry::mock());
    std::thread::spawn(move || {
        STORE_RUNTIME.handle().block_on(async {
            let builder = StoreBuilder::new(&LOGGER, &NODE_ID, &config, None, registry).await;
            let subscription_manager = builder.subscription_manager();
            let primary_pool = builder.primary_pool();

            let ident = ChainIdentifier {
                net_version: NETWORK_VERSION.to_owned(),
                genesis_block_hash: GENESIS_PTR.hash.clone(),
            };

            (
                builder.network_store(
                    vec![
                        (NETWORK_NAME.to_string(), ident.clone()),
                        (FAKE_NETWORK_SHARED.to_string(), ident),
                    ]
                    .into_iter()
                    .collect(),
                ),
                primary_pool,
                config,
                subscription_manager,
            )
        })
    })
    .join()
    .unwrap()
}

pub fn primary_connection() -> graph_store_postgres::layout_for_tests::Connection<'static> {
    let conn = PRIMARY_POOL.get().unwrap();
    graph_store_postgres::layout_for_tests::Connection::new(conn)
}

pub fn primary_mirror() -> graph_store_postgres::layout_for_tests::Mirror {
    let pool = PRIMARY_POOL.clone();
    let map = HashMap::from_iter(Some((PRIMARY_SHARD.clone(), pool)));
    graph_store_postgres::layout_for_tests::Mirror::new(&map)
}
