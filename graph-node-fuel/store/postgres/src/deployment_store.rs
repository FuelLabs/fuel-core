use detail::DeploymentDetail;
use diesel::connection::SimpleConnection;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use graph::anyhow::Context;
use graph::blockchain::block_stream::FirehoseCursor;
use graph::components::store::write::RowGroup;
use graph::components::store::{
    Batch, DerivedEntityQuery, PrunePhase, PruneReporter, PruneRequest, PruningStrategy,
    QueryPermit, StoredDynamicDataSource, VersionStats,
};
use graph::components::versions::VERSIONS;
use graph::data::query::Trace;
use graph::data::store::{Id, IdList};
use graph::data::subgraph::{status, SPEC_VERSION_0_0_6};
use graph::data_source::CausalityRegion;
use graph::prelude::futures03::FutureExt;
use graph::prelude::{
    ApiVersion, CancelHandle, CancelToken, CancelableError, EntityOperation, PoolWaitStats,
    SubgraphDeploymentEntity,
};
use graph::semver::Version;
use graph::tokio::task::JoinHandle;
use itertools::Itertools;
use lru_time_cache::LruCache;
use rand::{seq::SliceRandom, thread_rng};
use std::collections::{BTreeMap, HashMap};
use std::convert::Into;
use std::ops::Bound;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::{atomic::AtomicUsize, Arc, Mutex};
use std::time::{Duration, Instant};

use graph::components::store::EntityCollection;
use graph::components::subgraph::{ProofOfIndexingFinisher, ProofOfIndexingVersion};
use graph::constraint_violation;
use graph::data::subgraph::schema::{DeploymentCreate, SubgraphError};
use graph::prelude::{
    anyhow, debug, info, o, warn, web3, AttributeNames, BlockNumber, BlockPtr, CheapClone,
    DeploymentHash, DeploymentState, Entity, EntityQuery, Error, Logger, QueryExecutionError,
    StopwatchMetrics, StoreError, StoreEvent, UnfailOutcome, Value, ENV_VARS,
};
use graph::schema::{ApiSchema, EntityKey, EntityType, InputSchema};
use web3::types::Address;

use crate::block_range::{BLOCK_COLUMN, BLOCK_RANGE_COLUMN};
use crate::deployment::{self, OnSync};
use crate::detail::ErrorDetail;
use crate::dynds::DataSourcesTable;
use crate::primary::DeploymentId;
use crate::relational::index::{CreateIndex, Method};
use crate::relational::{Layout, LayoutCache, SqlName, Table};
use crate::relational_queries::FromEntityData;
use crate::{advisory_lock, catalog, retry};
use crate::{connection_pool::ConnectionPool, detail};
use crate::{dynds, primary::Site};

/// When connected to read replicas, this allows choosing which DB server to use for an operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReplicaId {
    /// The main server has write and read access.
    Main,

    /// A read replica identified by its index.
    ReadOnly(usize),
}

/// Commonly needed information about a subgraph that we cache in
/// `Store.subgraph_cache`. Only immutable subgraph data can be cached this
/// way as the cache lives for the lifetime of the `Store` object
#[derive(Clone)]
pub(crate) struct SubgraphInfo {
    /// The schema as supplied by the user
    pub(crate) input: InputSchema,
    /// The schema we derive from `input` with `graphql::schema::api::api_schema`
    pub(crate) api: HashMap<ApiVersion, Arc<ApiSchema>>,
    /// The block number at which this subgraph was grafted onto
    /// another one. We do not allow reverting past this block
    pub(crate) graft_block: Option<BlockNumber>,
    /// The deployment hash of the remote subgraph whose store
    /// will be GraphQL queried, for debugging purposes.
    pub(crate) debug_fork: Option<DeploymentHash>,
    pub(crate) description: Option<String>,
    pub(crate) repository: Option<String>,
    pub(crate) poi_version: ProofOfIndexingVersion,
    pub(crate) instrument: bool,
}

type PruneHandle = JoinHandle<Result<(), StoreError>>;

pub struct StoreInner {
    logger: Logger,

    pool: ConnectionPool,
    read_only_pools: Vec<ConnectionPool>,

    /// A list of the available replicas set up such that when we run
    /// through the list once, we picked each replica according to its
    /// desired weight. Each replica can appear multiple times in the list
    replica_order: Vec<ReplicaId>,
    /// The current position in `replica_order` so we know which one to
    /// pick next
    conn_round_robin_counter: AtomicUsize,

    /// A cache of commonly needed data about a subgraph.
    subgraph_cache: Mutex<LruCache<DeploymentHash, SubgraphInfo>>,

    /// A cache for the layout metadata for subgraphs. The Store just
    /// hosts this because it lives long enough, but it is managed from
    /// the entities module
    pub(crate) layout_cache: LayoutCache,

    prune_handles: Mutex<HashMap<DeploymentId, PruneHandle>>,
}

/// Storage of the data for individual deployments. Each `DeploymentStore`
/// corresponds to one of the database shards that `SubgraphStore` manages.
#[derive(Clone)]
pub struct DeploymentStore(Arc<StoreInner>);

impl CheapClone for DeploymentStore {}

impl Deref for DeploymentStore {
    type Target = StoreInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DeploymentStore {
    pub fn new(
        logger: &Logger,
        pool: ConnectionPool,
        read_only_pools: Vec<ConnectionPool>,
        mut pool_weights: Vec<usize>,
    ) -> Self {
        // Create a store-specific logger
        let logger = logger.new(o!("component" => "Store"));

        // Create a list of replicas with repetitions according to the weights
        // and shuffle the resulting list. Any missing weights in the list
        // default to 1
        pool_weights.resize(read_only_pools.len() + 1, 1);
        let mut replica_order: Vec<_> = pool_weights
            .iter()
            .enumerate()
            .flat_map(|(i, weight)| {
                let replica = if i == 0 {
                    ReplicaId::Main
                } else {
                    ReplicaId::ReadOnly(i - 1)
                };
                vec![replica; *weight]
            })
            .collect();
        let mut rng = thread_rng();
        replica_order.shuffle(&mut rng);
        debug!(logger, "Using postgres host order {:?}", replica_order);

        // Create the store
        let store = StoreInner {
            logger: logger.clone(),
            pool,
            read_only_pools,
            replica_order,
            conn_round_robin_counter: AtomicUsize::new(0),
            subgraph_cache: Mutex::new(LruCache::with_capacity(100)),
            layout_cache: LayoutCache::new(ENV_VARS.store.query_stats_refresh_interval),
            prune_handles: Mutex::new(HashMap::new()),
        };

        DeploymentStore(Arc::new(store))
    }

    pub(crate) fn create_deployment(
        &self,
        schema: &InputSchema,
        deployment: DeploymentCreate,
        site: Arc<Site>,
        graft_base: Option<Arc<Layout>>,
        replace: bool,
        on_sync: OnSync,
    ) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        conn.transaction(|| -> Result<_, StoreError> {
            let exists = deployment::exists(&conn, &site)?;

            // Create (or update) the metadata. Update only happens in tests
            let entities_with_causality_region =
                deployment.manifest.entities_with_causality_region.clone();
            if replace || !exists {
                deployment::create_deployment(&conn, &site, deployment, exists, replace)?;
            };

            // Create the schema for the subgraph data
            if !exists {
                let query = format!("create schema {}", &site.namespace);
                conn.batch_execute(&query)?;

                let layout = Layout::create_relational_schema(
                    &conn,
                    site.clone(),
                    schema,
                    entities_with_causality_region.into_iter().collect(),
                )?;
                // See if we are grafting and check that the graft is permissible
                if let Some(base) = graft_base {
                    let errors = layout.can_copy_from(&base);
                    if !errors.is_empty() {
                        return Err(StoreError::Unknown(anyhow!(
                            "The subgraph `{}` cannot be used as the graft base \
                             for `{}` because the schemas are incompatible:\n    - {}",
                            &base.catalog.site.namespace,
                            &layout.catalog.site.namespace,
                            errors.join("\n    - ")
                        )));
                    }
                }

                // Create data sources table
                if site.schema_version.private_data_sources() {
                    conn.batch_execute(&DataSourcesTable::new(site.namespace.clone()).as_ddl())?;
                }
            }

            deployment::set_on_sync(&conn, &site, on_sync)?;

            Ok(())
        })
    }

    pub(crate) fn load_deployment(
        &self,
        site: Arc<Site>,
    ) -> Result<SubgraphDeploymentEntity, StoreError> {
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site.clone())?;
        Ok(
            detail::deployment_entity(&conn, &site, &layout.input_schema)
                .with_context(|| format!("Deployment details not found for {}", site.deployment))?,
        )
    }

    // Remove the data and metadata for the deployment `site`. This operation
    // is not reversible
    pub(crate) fn drop_deployment(&self, site: &Site) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        conn.transaction(|| {
            crate::deployment::drop_schema(&conn, &site.namespace)?;
            if !site.schema_version.private_data_sources() {
                crate::dynds::shared::drop(&conn, &site.deployment)?;
            }
            crate::deployment::drop_metadata(&conn, site)
        })
    }

    pub(crate) fn execute_query<T: FromEntityData>(
        &self,
        conn: &PgConnection,
        site: Arc<Site>,
        query: EntityQuery,
    ) -> Result<(Vec<T>, Trace), QueryExecutionError> {
        let layout = self.layout(conn, site)?;

        let logger = query
            .logger
            .cheap_clone()
            .unwrap_or_else(|| self.logger.cheap_clone());
        layout.query(&logger, conn, query)
    }

    fn check_interface_entity_uniqueness(
        &self,
        conn: &PgConnection,
        layout: &Layout,
        entity_type: &EntityType,
        entity_id: &Id,
    ) -> Result<(), StoreError> {
        // Collect all types that share an interface implementation with this
        // entity type, and make sure there are no conflicting IDs.
        //
        // To understand why this is necessary, suppose that `Dog` and `Cat` are
        // types and both implement an interface `Pet`, and both have instances
        // with `id: "Fred"`. If a type `PetOwner` has a field `pets: [Pet]`
        // then with the value `pets: ["Fred"]`, there's no way to disambiguate
        // if that's Fred the Dog, Fred the Cat or both.
        //
        // This assumes that there are no concurrent writes to a subgraph.
        let entity_type_str = entity_type.to_string();
        let types_with_shared_interface = entity_type.share_interfaces()?;

        if !types_with_shared_interface.is_empty() {
            if let Some(conflicting_entity) =
                layout.conflicting_entity(conn, entity_id, types_with_shared_interface)?
            {
                return Err(StoreError::ConflictingId(
                    entity_type_str,
                    entity_id.to_string(),
                    conflicting_entity,
                ));
            }
        }
        Ok(())
    }

    fn apply_entity_modifications<'a>(
        &self,
        conn: &PgConnection,
        layout: &Layout,
        groups: impl Iterator<Item = &'a RowGroup>,
        stopwatch: &StopwatchMetrics,
    ) -> Result<i32, StoreError> {
        let mut count = 0;

        for group in groups {
            count += group.entity_count_change();

            // Clamp entities before inserting them to avoid having versions
            // with overlapping block ranges
            let section = stopwatch.start_section("apply_entity_modifications_delete");
            layout.delete(conn, group, stopwatch)?;
            section.end();

            let section = stopwatch.start_section("check_interface_entity_uniqueness");
            for row in group.writes().filter(|emod| emod.creates_entity()) {
                // WARNING: This will potentially execute 2 queries for each entity key.
                self.check_interface_entity_uniqueness(conn, layout, &group.entity_type, row.id())?;
            }
            section.end();

            let section = stopwatch.start_section("apply_entity_modifications_insert");
            layout.insert(conn, group, stopwatch)?;
            section.end();
        }

        Ok(count)
    }

    /// Execute a closure with a connection to the database.
    ///
    /// # API
    ///   The API of using a closure to bound the usage of the connection serves several
    ///   purposes:
    ///
    ///   * Moves blocking database access out of the `Future::poll`. Within
    ///     `Future::poll` (which includes all `async` methods) it is illegal to
    ///     perform a blocking operation. This includes all accesses to the
    ///     database, acquiring of locks, etc. Calling a blocking operation can
    ///     cause problems with `Future` combinators (including but not limited
    ///     to select, timeout, and FuturesUnordered) and problems with
    ///     executors/runtimes. This method moves the database work onto another
    ///     thread in a way which does not block `Future::poll`.
    ///
    ///   * Limit the total number of connections. Because the supplied closure
    ///     takes a reference, we know the scope of the usage of all entity
    ///     connections and can limit their use in a non-blocking way.
    ///
    /// # Cancellation
    ///   The normal pattern for futures in Rust is drop to cancel. Once we
    ///   spawn the database work in a thread though, this expectation no longer
    ///   holds because the spawned task is the independent of this future. So,
    ///   this method provides a cancel token which indicates that the `Future`
    ///   has been dropped. This isn't *quite* as good as drop on cancel,
    ///   because a drop on cancel can do things like cancel http requests that
    ///   are in flight, but checking for cancel periodically is a significant
    ///   improvement.
    ///
    ///   The implementation of the supplied closure should check for cancel
    ///   between every operation that is potentially blocking. This includes
    ///   any method which may interact with the database. The check can be
    ///   conveniently written as `token.check_cancel()?;`. It is low overhead
    ///   to check for cancel, so when in doubt it is better to have too many
    ///   checks than too few.
    ///
    /// # Panics:
    ///   * This task will panic if the supplied closure panics
    ///   * This task will panic if the supplied closure returns Err(Cancelled)
    ///     when the supplied cancel token is not cancelled.
    pub(crate) async fn with_conn<T: Send + 'static>(
        &self,
        f: impl 'static
            + Send
            + FnOnce(
                &PooledConnection<ConnectionManager<PgConnection>>,
                &CancelHandle,
            ) -> Result<T, CancelableError<StoreError>>,
    ) -> Result<T, StoreError> {
        self.pool.with_conn(f).await
    }

    /// Deprecated. Use `with_conn` instead.
    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, StoreError> {
        self.pool.get()
    }

    /// Panics if `idx` is not a valid index for a read only pool.
    fn read_only_conn(
        &self,
        idx: usize,
    ) -> Result<PooledConnection<ConnectionManager<PgConnection>>, Error> {
        self.read_only_pools[idx].get().map_err(Error::from)
    }

    pub(crate) fn get_replica_conn(
        &self,
        replica: ReplicaId,
    ) -> Result<PooledConnection<ConnectionManager<PgConnection>>, Error> {
        let conn = match replica {
            ReplicaId::Main => self.get_conn()?,
            ReplicaId::ReadOnly(idx) => self.read_only_conn(idx)?,
        };
        Ok(conn)
    }

    pub(crate) async fn query_permit(&self, replica: ReplicaId) -> Result<QueryPermit, StoreError> {
        let pool = match replica {
            ReplicaId::Main => &self.pool,
            ReplicaId::ReadOnly(idx) => &self.read_only_pools[idx],
        };
        pool.query_permit().await
    }

    pub(crate) fn wait_stats(&self, replica: ReplicaId) -> Result<PoolWaitStats, StoreError> {
        match replica {
            ReplicaId::Main => self.pool.wait_stats(),
            ReplicaId::ReadOnly(idx) => self.read_only_pools[idx].wait_stats(),
        }
    }

    /// Return the layout for a deployment. Since constructing a `Layout`
    /// object takes a bit of computation, we cache layout objects that do
    /// not have a pending migration in the Store, i.e., for the lifetime of
    /// the Store. Layout objects with a pending migration can not be
    /// cached for longer than a transaction since they might change
    /// without us knowing
    pub(crate) fn layout(
        &self,
        conn: &PgConnection,
        site: Arc<Site>,
    ) -> Result<Arc<Layout>, StoreError> {
        self.layout_cache.get(&self.logger, conn, site)
    }

    /// Return the layout for a deployment. This might use a database
    /// connection for the lookup and should only be called if the caller
    /// does not have a connection currently. If it does, use `layout`
    pub(crate) fn find_layout(&self, site: Arc<Site>) -> Result<Arc<Layout>, StoreError> {
        if let Some(layout) = self.layout_cache.find(site.as_ref()) {
            return Ok(layout);
        }

        let conn = self.get_conn()?;
        self.layout(&conn, site)
    }

    fn subgraph_info_with_conn(
        &self,
        conn: &PgConnection,
        site: &Site,
    ) -> Result<SubgraphInfo, StoreError> {
        if let Some(info) = self.subgraph_cache.lock().unwrap().get(&site.deployment) {
            return Ok(info.clone());
        }

        let manifest_info = deployment::ManifestInfo::load(conn, site)?;

        let graft_block =
            deployment::graft_point(conn, &site.deployment)?.map(|(_, ptr)| ptr.number);

        let debug_fork = deployment::debug_fork(conn, &site.deployment)?;

        // Generate an API schema for the subgraph and make sure all types in the
        // API schema have a @subgraphId directive as well
        let mut api: HashMap<ApiVersion, Arc<ApiSchema>> = HashMap::new();

        for version in VERSIONS.iter() {
            let api_version = ApiVersion::from_version(version).expect("Invalid API version");
            let schema = manifest_info.input_schema.api_schema()?;
            api.insert(api_version, Arc::new(schema));
        }

        let spec_version =
            Version::from_str(&manifest_info.spec_version).map_err(anyhow::Error::from)?;
        let poi_version = if spec_version.ge(&SPEC_VERSION_0_0_6) {
            ProofOfIndexingVersion::Fast
        } else {
            ProofOfIndexingVersion::Legacy
        };

        let info = SubgraphInfo {
            input: manifest_info.input_schema,
            api,
            graft_block,
            debug_fork,
            description: manifest_info.description,
            repository: manifest_info.repository,
            poi_version,
            instrument: manifest_info.instrument,
        };

        // Insert the schema into the cache.
        let mut cache = self.subgraph_cache.lock().unwrap();
        cache.insert(site.deployment.clone(), info);

        Ok(cache.get(&site.deployment).unwrap().clone())
    }

    pub(crate) fn subgraph_info(&self, site: &Site) -> Result<SubgraphInfo, StoreError> {
        if let Some(info) = self.subgraph_cache.lock().unwrap().get(&site.deployment) {
            return Ok(info.clone());
        }

        let conn = self.get_conn()?;
        self.subgraph_info_with_conn(&conn, site)
    }

    fn block_ptr_with_conn(
        conn: &PgConnection,
        site: Arc<Site>,
    ) -> Result<Option<BlockPtr>, StoreError> {
        deployment::block_ptr(conn, &site.deployment)
    }

    pub(crate) fn deployment_details(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<DeploymentDetail>, StoreError> {
        let conn = self.get_conn()?;
        conn.transaction(|| -> Result<_, StoreError> { detail::deployment_details(&conn, ids) })
    }

    pub(crate) fn deployment_statuses(
        &self,
        sites: &[Arc<Site>],
    ) -> Result<Vec<status::Info>, StoreError> {
        let conn = self.get_conn()?;
        conn.transaction(|| -> Result<Vec<status::Info>, StoreError> {
            detail::deployment_statuses(&conn, sites)
        })
    }

    pub(crate) fn deployment_exists_and_synced(
        &self,
        id: &DeploymentHash,
    ) -> Result<bool, StoreError> {
        let conn = self.get_conn()?;
        deployment::exists_and_synced(&conn, id.as_str())
    }

    pub(crate) fn deployment_synced(&self, id: &DeploymentHash) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        conn.transaction(|| deployment::set_synced(&conn, id))
    }

    /// Look up the on_sync action for this deployment
    pub(crate) fn on_sync(&self, site: &Site) -> Result<OnSync, StoreError> {
        let conn = self.get_conn()?;
        deployment::on_sync(&conn, site.id)
    }

    /// Return the source if `site` or `None` if `site` is neither a graft
    /// nor a copy
    pub(crate) fn source_of_copy(&self, site: &Site) -> Result<Option<DeploymentId>, StoreError> {
        let conn = self.get_conn()?;
        crate::copy::source(&conn, site)
    }

    // Only used for tests
    #[cfg(debug_assertions)]
    pub(crate) fn drop_deployment_schema(
        &self,
        namespace: &crate::primary::Namespace,
    ) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        deployment::drop_schema(&conn, namespace)
    }

    // Only used for tests
    #[cfg(debug_assertions)]
    pub(crate) fn drop_all_metadata(&self) -> Result<(), StoreError> {
        // Delete metadata entities in each shard

        // This needs to touch all the tables in the subgraphs schema
        const QUERY: &str = "
        delete from subgraphs.dynamic_ethereum_contract_data_source;
        delete from subgraphs.subgraph;
        delete from subgraphs.subgraph_deployment;
        delete from subgraphs.subgraph_deployment_assignment;
        delete from subgraphs.subgraph_version;
        delete from subgraphs.subgraph_manifest;
        delete from subgraphs.copy_table_state;
        delete from subgraphs.copy_state;
        delete from active_copies;
    ";

        let conn = self.get_conn()?;
        conn.batch_execute(QUERY)?;
        conn.batch_execute("delete from deployment_schemas;")?;
        Ok(())
    }

    pub(crate) async fn vacuum(&self) -> Result<(), StoreError> {
        self.with_conn(|conn, _| {
            conn.batch_execute("vacuum (analyze) subgraphs.subgraph_deployment")?;
            Ok(())
        })
        .await
    }

    /// Runs the SQL `ANALYZE` command in a table.
    pub(crate) fn analyze(&self, site: Arc<Site>, entity: Option<&str>) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site)?;
        let tables = entity
            .map(|entity| resolve_table_name(&layout, entity))
            .transpose()?
            .map(|table| vec![table])
            .unwrap_or_else(|| layout.tables.values().map(Arc::as_ref).collect());
        for table in tables {
            table.analyze(&conn)?;
        }
        Ok(())
    }

    pub(crate) fn stats_targets(
        &self,
        site: Arc<Site>,
    ) -> Result<(i32, BTreeMap<SqlName, BTreeMap<SqlName, i32>>), StoreError> {
        let conn = self.get_conn()?;
        let default = catalog::default_stats_target(&conn)?;
        let targets = catalog::stats_targets(&conn, &site.namespace)?;

        Ok((default, targets))
    }

    pub(crate) fn set_stats_target(
        &self,
        site: Arc<Site>,
        entity: Option<&str>,
        columns: Vec<String>,
        target: i32,
    ) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site.clone())?;

        let tables = entity
            .map(|entity| resolve_table_name(&layout, entity))
            .transpose()?
            .map(|table| vec![table])
            .unwrap_or_else(|| layout.tables.values().map(Arc::as_ref).collect());

        conn.transaction(|| {
            for table in tables {
                let (columns, _) = resolve_column_names_and_index_exprs(table, &columns)?;

                catalog::set_stats_target(&conn, &site.namespace, &table.name, &columns, target)?;
            }
            Ok(())
        })
    }

    /// Runs the SQL `ANALYZE` command in a table, with a shared connection.
    pub(crate) fn analyze_with_conn(
        &self,
        site: Arc<Site>,
        entity_name: &str,
        conn: &PgConnection,
    ) -> Result<(), StoreError> {
        let store = self.clone();
        let entity_name = entity_name.to_owned();
        let layout = store.layout(conn, site)?;
        let table = resolve_table_name(&layout, &entity_name)?;
        table.analyze(conn)
    }

    /// Creates a new index in the specified Entity table if it doesn't already exist.
    ///
    /// This is a potentially time-consuming operation.
    pub(crate) async fn create_manual_index(
        &self,
        site: Arc<Site>,
        entity_name: &str,
        field_names: Vec<String>,
        index_method: Method,
        after: Option<BlockNumber>,
    ) -> Result<(), StoreError> {
        let store = self.clone();
        let entity_name = entity_name.to_owned();
        self.with_conn(move |conn, _| {
            let schema_name = site.namespace.clone();
            let layout = store.layout(conn, site)?;
            let table = resolve_table_name(&layout, &entity_name)?;
            let (column_names, index_exprs) =
                resolve_column_names_and_index_exprs(table, &field_names)?;

            let column_names_sep_by_underscores = column_names.join("_");
            let index_exprs_joined = index_exprs.join(", ");
            let table_name = &table.name;
            let index_name = format!(
                "manual_{table_name}_{column_names_sep_by_underscores}{}",
                after.map_or_else(String::new, |a| format!("_{}", a))
            );

            let mut sql = format!(
                "create index concurrently if not exists {index_name} \
                 on {schema_name}.{table_name} using {index_method} \
                 ({index_exprs_joined}) ",
            );

            // If 'after' is provided and the table is not immutable, add a WHERE clause for partial indexing
            if let Some(after) = after {
                if !table.immutable {
                    sql.push_str(&format!(
                        " where coalesce(upper({}), 2147483647) > {}",
                        BLOCK_RANGE_COLUMN, after
                    ));
                } else {
                    return Err(CancelableError::Error(StoreError::Unknown(anyhow!(
                        "Partial index not allowed on immutable table `{}`",
                        table_name
                    ))));
                }
            }

            // This might take a long time.
            conn.execute(&sql)?;
            // check if the index creation was successfull
            let index_is_valid =
                catalog::check_index_is_valid(conn, schema_name.as_str(), &index_name)?;
            if index_is_valid {
                Ok(())
            } else {
                // Index creation falied. We should drop the index before returning.
                let drop_index_sql =
                    format!("drop index concurrently if exists {schema_name}.{index_name}");
                conn.execute(&drop_index_sql)?;
                Err(StoreError::Canceled)
            }
            .map_err(Into::into)
        })
        .await
    }

    /// Returns a list of all existing indexes for the specified Entity table.
    pub(crate) async fn indexes_for_entity(
        &self,
        site: Arc<Site>,
        entity_name: &str,
    ) -> Result<Vec<CreateIndex>, StoreError> {
        let store = self.clone();
        let entity_name = entity_name.to_owned();
        self.with_conn(move |conn, _| {
            let schema_name = site.namespace.clone();
            let layout = store.layout(conn, site)?;
            let table = resolve_table_name(&layout, &entity_name)?;
            let table_name = &table.name;
            let indexes =
                catalog::indexes_for_table(conn, schema_name.as_str(), table_name.as_str())
                    .map_err(StoreError::from)?;
            Ok(indexes.into_iter().map(CreateIndex::parse).collect())
        })
        .await
    }

    /// Drops an index for a given deployment, concurrently.
    pub(crate) async fn drop_index(
        &self,
        site: Arc<Site>,
        index_name: &str,
    ) -> Result<(), StoreError> {
        let index_name = String::from(index_name);
        self.with_conn(move |conn, _| {
            let schema_name = site.namespace.clone();
            catalog::drop_index(conn, schema_name.as_str(), &index_name).map_err(Into::into)
        })
        .await
    }

    pub(crate) async fn set_account_like(
        &self,
        site: Arc<Site>,
        table: &str,
        is_account_like: bool,
    ) -> Result<(), StoreError> {
        let store = self.clone();
        let table = table.to_string();
        self.with_conn(move |conn, _| {
            let layout = store.layout(conn, site.clone())?;
            let table = resolve_table_name(&layout, &table)?;
            catalog::set_account_like(conn, &site, &table.name, is_account_like).map_err(Into::into)
        })
        .await
    }

    pub(crate) fn set_history_blocks(
        &self,
        site: &Site,
        history_blocks: BlockNumber,
        reorg_threshold: BlockNumber,
    ) -> Result<(), StoreError> {
        if history_blocks <= reorg_threshold {
            return Err(constraint_violation!(
                "the amount of history to keep for sgd{} can not be set to \
                 {history_blocks} since it must be more than the \
                 reorg threshold {reorg_threshold}",
                site.id
            ));
        }

        // Invalidate the layout cache for this site so that the next access
        // will use the updated value
        self.layout_cache.remove(site);

        let conn = self.get_conn()?;
        deployment::set_history_blocks(&conn, site, history_blocks)
    }

    pub(crate) async fn prune(
        self: &Arc<Self>,
        reporter: Box<dyn PruneReporter>,
        site: Arc<Site>,
        req: PruneRequest,
    ) -> Result<Box<dyn PruneReporter>, StoreError> {
        fn do_prune(
            store: Arc<DeploymentStore>,
            conn: &PooledConnection<ConnectionManager<PgConnection>>,
            site: Arc<Site>,
            cancel: &CancelHandle,
            req: PruneRequest,
            mut reporter: Box<dyn PruneReporter>,
        ) -> Result<Box<dyn PruneReporter>, CancelableError<StoreError>> {
            let layout = store.layout(conn, site.clone())?;
            cancel.check_cancel()?;
            let state = deployment::state(conn, site.deployment.clone())?;

            if state.latest_block.number <= req.history_blocks {
                // We haven't accumulated enough history yet, nothing to prune
                return Ok(reporter);
            }

            if state.earliest_block_number > req.earliest_block {
                // We already have less history than we need (e.g., because
                // of a manual onetime prune), nothing to prune
                return Ok(reporter);
            }

            conn.transaction(|| {
                deployment::set_earliest_block(conn, site.as_ref(), req.earliest_block)
            })?;

            cancel.check_cancel()?;

            layout.prune(&store.logger, reporter.as_mut(), conn, &req, cancel)?;
            Ok(reporter)
        }

        let store = self.clone();
        self.with_conn(move |conn, cancel| {
            // We lock pruning for this deployment to make sure that if the
            // deployment is reassigned to another node, that node won't
            // kick off a pruning run while this node might still be pruning
            if advisory_lock::try_lock_pruning(conn, &site)? {
                let res = do_prune(store, conn, site.cheap_clone(), cancel, req, reporter);
                advisory_lock::unlock_pruning(conn, &site)?;
                res
            } else {
                Ok(reporter)
            }
        })
        .await
    }
}

/// Methods that back the trait `graph::components::Store`, but have small
/// variations in their signatures
impl DeploymentStore {
    pub(crate) async fn block_ptr(&self, site: Arc<Site>) -> Result<Option<BlockPtr>, StoreError> {
        let site = site.cheap_clone();

        self.with_conn(|conn, cancel| {
            cancel.check_cancel()?;

            Self::block_ptr_with_conn(conn, site).map_err(Into::into)
        })
        .await
    }

    pub(crate) async fn block_cursor(&self, site: Arc<Site>) -> Result<FirehoseCursor, StoreError> {
        let site = site.cheap_clone();

        self.with_conn(|conn, cancel| {
            cancel.check_cancel()?;

            deployment::get_subgraph_firehose_cursor(conn, site)
                .map(FirehoseCursor::from)
                .map_err(Into::into)
        })
        .await
    }

    pub(crate) async fn supports_proof_of_indexing<'a>(
        &self,
        site: Arc<Site>,
    ) -> Result<bool, StoreError> {
        let store = self.clone();
        self.with_conn(move |conn, cancel| {
            cancel.check_cancel()?;
            let layout = store.layout(conn, site)?;
            Ok(layout.supports_proof_of_indexing())
        })
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn get_proof_of_indexing(
        &self,
        site: Arc<Site>,
        indexer: &Option<Address>,
        block: BlockPtr,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        let indexer = *indexer;
        let site2 = site.cheap_clone();
        let store = self.cheap_clone();
        let info = self.subgraph_info(&site)?;
        let poi_digest = info.input.poi_digest();

        let entities: Option<(Vec<Entity>, BlockPtr)> = self
            .with_conn(move |conn, cancel| {
                let site = site.clone();
                cancel.check_cancel()?;

                let layout = store.layout(conn, site.cheap_clone())?;

                if !layout.supports_proof_of_indexing() {
                    return Ok(None);
                }

                conn.transaction::<_, CancelableError<anyhow::Error>, _>(move || {
                    let mut block_ptr = block.cheap_clone();
                    let latest_block_ptr =
                        match Self::block_ptr_with_conn(conn, site.cheap_clone())? {
                            Some(inner) => inner,
                            None => return Ok(None),
                        };

                    cancel.check_cancel()?;

                    // FIXME: (Determinism)
                    //
                    // It is vital to ensure that the block hash given in the query
                    // is a parent of the latest block indexed for the subgraph.
                    // Unfortunately the machinery needed to do this is not yet in place.
                    // The best we can do right now is just to make sure that the block number
                    // is high enough.
                    if latest_block_ptr.number < block.number {
                        // If a subgraph has failed deterministically then any blocks past head
                        // should return the same POI
                        let fatal_error = ErrorDetail::fatal(conn, &site.deployment)?;
                        block_ptr = match fatal_error {
                            Some(se) => TryInto::<SubgraphError>::try_into(se)?
                                .block_ptr
                                .unwrap_or(block_ptr),
                            None => return Ok(None),
                        };
                    };

                    let query = EntityQuery::new(
                        site.deployment.cheap_clone(),
                        block_ptr.number,
                        EntityCollection::All(vec![(
                            info.input.poi_type().clone(),
                            AttributeNames::All,
                        )]),
                    );
                    let entities = store
                        .execute_query::<Entity>(conn, site, query)
                        .map(|(entities, _)| entities)
                        .map_err(anyhow::Error::from)?;

                    Ok(Some((entities, block_ptr)))
                })
                .map_err(Into::into)
            })
            .await?;

        let (entities, block_ptr) = if let Some((entities, bp)) = entities {
            (entities, bp)
        } else {
            return Ok(None);
        };

        let mut by_causality_region = entities
            .into_iter()
            .map(|e| {
                let causality_region = e.id();
                let digest = match e.get(poi_digest.as_str()) {
                    Some(Value::Bytes(b)) => Ok(b.clone()),
                    other => Err(anyhow::anyhow!(
                        "Entity has non-bytes digest attribute: {:?}",
                        other
                    )),
                }?;

                Ok((causality_region, digest))
            })
            .collect::<Result<HashMap<_, _>, anyhow::Error>>()?;

        let mut finisher =
            ProofOfIndexingFinisher::new(&block_ptr, &site2.deployment, &indexer, info.poi_version);
        for (name, region) in by_causality_region.drain() {
            finisher.add_causality_region(&name, &region);
        }

        Ok(Some(finisher.finish()))
    }

    /// Get the entity matching `key` from the deployment `site`. Only
    /// consider entities as of the given `block`
    pub(crate) fn get(
        &self,
        site: Arc<Site>,
        key: &EntityKey,
        block: BlockNumber,
    ) -> Result<Option<Entity>, StoreError> {
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site)?;
        layout.find(&conn, key, block)
    }

    /// Retrieve all the entities matching `ids_for_type`, both the type and causality region, from
    /// the deployment `site`. Only consider entities as of the given `block`
    pub(crate) fn get_many(
        &self,
        site: Arc<Site>,
        ids_for_type: &BTreeMap<(EntityType, CausalityRegion), IdList>,
        block: BlockNumber,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        if ids_for_type.is_empty() {
            return Ok(BTreeMap::new());
        }
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site)?;

        layout.find_many(&conn, ids_for_type, block)
    }

    pub(crate) fn get_derived(
        &self,
        site: Arc<Site>,
        derived_query: &DerivedEntityQuery,
        block: BlockNumber,
        excluded_keys: &Vec<EntityKey>,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site)?;
        layout.find_derived(&conn, derived_query, block, excluded_keys)
    }

    pub(crate) fn get_changes(
        &self,
        site: Arc<Site>,
        block: BlockNumber,
    ) -> Result<Vec<EntityOperation>, StoreError> {
        let conn = self.get_conn()?;
        let layout = self.layout(&conn, site)?;
        let changes = layout.find_changes(&conn, block)?;

        Ok(changes)
    }

    // Only used by tests
    #[cfg(debug_assertions)]
    pub(crate) fn find(
        &self,
        site: Arc<Site>,
        query: EntityQuery,
    ) -> Result<Vec<Entity>, QueryExecutionError> {
        let conn = self.get_conn()?;
        self.execute_query(&conn, site, query)
            .map(|(entities, _)| entities)
    }

    pub(crate) fn transact_block_operations(
        self: &Arc<Self>,
        logger: &Logger,
        site: Arc<Site>,
        batch: &Batch,
        stopwatch: &StopwatchMetrics,
        manifest_idx_and_name: &[(u32, String)],
    ) -> Result<StoreEvent, StoreError> {
        let conn = {
            let _section = stopwatch.start_section("transact_blocks_get_conn");
            self.get_conn()?
        };

        // Emit a store event for the changes we are about to make. We
        // wait with sending it until we have done all our other work
        // so that we do not hold a lock on the notification queue
        // for longer than we have to
        let event: StoreEvent = batch.store_event(&site.deployment);

        let (layout, earliest_block) = deployment::with_lock(&conn, &site, || {
            conn.transaction(|| -> Result<_, StoreError> {
                // Make the changes
                let layout = self.layout(&conn, site.clone())?;

                let section = stopwatch.start_section("apply_entity_modifications");
                let count = self.apply_entity_modifications(
                    &conn,
                    layout.as_ref(),
                    batch.groups(),
                    stopwatch,
                )?;
                section.end();

                dynds::insert(&conn, &site, &batch.data_sources, manifest_idx_and_name)?;

                dynds::update_offchain_status(&conn, &site, &batch.offchain_to_remove)?;

                if !batch.deterministic_errors.is_empty() {
                    deployment::insert_subgraph_errors(
                        &conn,
                        &site.deployment,
                        &batch.deterministic_errors,
                        batch.block_ptr.number,
                    )?;

                    if batch.is_non_fatal_errors_active {
                        deployment::update_non_fatal_errors(
                            &conn,
                            &site.deployment,
                            deployment::SubgraphHealth::Unhealthy,
                            Some(&batch.deterministic_errors),
                        )?;
                    }
                }

                let earliest_block = deployment::transact_block(
                    &conn,
                    &site,
                    &batch.block_ptr,
                    &batch.firehose_cursor,
                    count,
                )?;

                Ok((layout, earliest_block))
            })
        })?;

        if batch.block_ptr.number as f64
            > earliest_block as f64
                + layout.history_blocks as f64 * ENV_VARS.store.history_slack_factor
        {
            // This only measures how long it takes to spawn pruning, not
            // how long pruning itself takes
            let _section = stopwatch.start_section("transact_blocks_prune");

            self.spawn_prune(
                logger,
                site,
                layout.history_blocks,
                earliest_block,
                batch.block_ptr.number,
            )?;
        }

        Ok(event)
    }

    fn spawn_prune(
        self: &Arc<Self>,
        logger: &Logger,
        site: Arc<Site>,
        history_blocks: BlockNumber,
        earliest_block: BlockNumber,
        latest_block: BlockNumber,
    ) -> Result<(), StoreError> {
        fn prune_in_progress(store: &DeploymentStore, site: &Site) -> Result<bool, StoreError> {
            let finished = store
                .prune_handles
                .lock()
                .unwrap()
                .get(&site.id)
                .map(|handle| handle.is_finished());
            match finished {
                Some(true) => {
                    // A previous prune has finished
                    let handle = store
                        .prune_handles
                        .lock()
                        .unwrap()
                        .remove(&site.id)
                        .unwrap();
                    match FutureExt::now_or_never(handle) {
                        Some(Ok(Ok(()))) => Ok(false),
                        Some(Ok(Err(err))) => Err(StoreError::PruneFailure(err.to_string())),
                        Some(Err(join_err)) => Err(StoreError::PruneFailure(join_err.to_string())),
                        None => Err(constraint_violation!(
                            "prune handle is finished but not ready"
                        )),
                    }
                }
                Some(false) => {
                    // A previous prune is still in progress
                    Ok(true)
                }
                None => {
                    // There is no prune in progress
                    Ok(false)
                }
            }
        }

        async fn run(
            logger: Logger,
            store: Arc<DeploymentStore>,
            site: Arc<Site>,
            req: PruneRequest,
        ) -> Result<(), StoreError> {
            let logger2 = logger.cheap_clone();
            retry::forever_async(&logger2, "prune", move || {
                let store = store.cheap_clone();
                let reporter = OngoingPruneReporter::new(logger.cheap_clone());
                let site = site.cheap_clone();
                async move { store.prune(reporter, site, req).await.map(|_| ()) }
            })
            .await
        }

        if !prune_in_progress(&self, &site)? {
            let req = PruneRequest::new(
                &site.as_ref().into(),
                history_blocks,
                ENV_VARS.reorg_threshold,
                earliest_block,
                latest_block,
            )?;

            let deployment_id = site.id;
            let handle = graph::spawn(run(logger.cheap_clone(), self.clone(), site, req));
            self.prune_handles
                .lock()
                .unwrap()
                .insert(deployment_id, handle);
        }
        Ok(())
    }

    fn rewind_or_truncate_with_conn(
        &self,
        conn: &PgConnection,
        site: Arc<Site>,
        block_ptr_to: BlockPtr,
        firehose_cursor: &FirehoseCursor,
        truncate: bool,
    ) -> Result<StoreEvent, StoreError> {
        let event = deployment::with_lock(conn, &site, || {
            conn.transaction(|| -> Result<_, StoreError> {
                // Don't revert past a graft point
                let info = self.subgraph_info_with_conn(conn, site.as_ref())?;
                if let Some(graft_block) = info.graft_block {
                    if graft_block > block_ptr_to.number {
                        return Err(anyhow!(
                            "Can not revert subgraph `{}` to block {} as it was \
                        grafted at block {} and reverting past a graft point \
                        is not possible",
                            site.deployment.clone(),
                            block_ptr_to.number,
                            graft_block
                        )
                        .into());
                    }
                }

                // The revert functions want the number of the first block that we need to get rid of
                let block = block_ptr_to.number + 1;

                deployment::revert_block_ptr(
                    conn,
                    &site.deployment,
                    block_ptr_to,
                    firehose_cursor,
                )?;

                // Revert the data
                let layout = self.layout(conn, site.clone())?;

                let event = if truncate {
                    let event = layout.truncate_tables(conn)?;
                    deployment::set_entity_count(conn, site.as_ref(), layout.count_query.as_str())?;
                    event
                } else {
                    let (event, count) = layout.revert_block(conn, block)?;
                    deployment::update_entity_count(conn, site.as_ref(), count)?;
                    event
                };

                // Revert the meta data changes that correspond to this subgraph.
                // Only certain meta data changes need to be reverted, most
                // importantly creation of dynamic data sources. We ensure in the
                // rest of the code that we only record history for those meta data
                // changes that might need to be reverted
                Layout::revert_metadata(conn, &site, block)?;

                Ok(event)
            })
        })?;

        Ok(event)
    }

    pub(crate) fn truncate(
        &self,
        site: Arc<Site>,
        block_ptr_to: BlockPtr,
    ) -> Result<StoreEvent, StoreError> {
        let conn = self.get_conn()?;

        // Unwrap: If we are reverting then the block ptr is not `None`.
        let block_ptr_from = Self::block_ptr_with_conn(&conn, site.cheap_clone())?.unwrap();

        // Sanity check on block numbers
        if block_ptr_from.number <= block_ptr_to.number {
            constraint_violation!(
                "truncate must go backwards, but would go from block {} to block {}",
                block_ptr_from.number,
                block_ptr_to.number
            );
        }

        // When rewinding, we reset the firehose cursor. That way, on resume, Firehose will start
        // from the block_ptr instead (with sanity check to ensure it's resume at the exact block).
        self.rewind_or_truncate_with_conn(&conn, site, block_ptr_to, &FirehoseCursor::None, true)
    }

    pub(crate) fn rewind(
        &self,
        site: Arc<Site>,
        block_ptr_to: BlockPtr,
    ) -> Result<StoreEvent, StoreError> {
        let conn = self.get_conn()?;

        // Unwrap: If we are reverting then the block ptr is not `None`.
        let block_ptr_from = Self::block_ptr_with_conn(&conn, site.cheap_clone())?.unwrap();

        // Sanity check on block numbers
        if block_ptr_from.number <= block_ptr_to.number {
            constraint_violation!(
                "rewind must go backwards, but would go from block {} to block {}",
                block_ptr_from.number,
                block_ptr_to.number
            );
        }

        // When rewinding, we reset the firehose cursor. That way, on resume, Firehose will start
        // from the block_ptr instead (with sanity check to ensure it's resume at the exact block).
        self.rewind_or_truncate_with_conn(&conn, site, block_ptr_to, &FirehoseCursor::None, false)
    }

    pub(crate) fn revert_block_operations(
        &self,
        site: Arc<Site>,
        block_ptr_to: BlockPtr,
        firehose_cursor: &FirehoseCursor,
    ) -> Result<StoreEvent, StoreError> {
        let conn = self.get_conn()?;
        // Unwrap: If we are reverting then the block ptr is not `None`.
        let deployment_head = Self::block_ptr_with_conn(&conn, site.cheap_clone())?.unwrap();

        // Confidence check on revert to ensure we go backward only
        if block_ptr_to.number >= deployment_head.number {
            panic!("revert_block_operations must revert only backward, you are trying to revert forward going from subgraph block {} to new block {}", deployment_head, block_ptr_to);
        }

        self.rewind_or_truncate_with_conn(&conn, site, block_ptr_to, firehose_cursor, false)
    }

    pub(crate) async fn deployment_state_from_id(
        &self,
        id: DeploymentHash,
    ) -> Result<DeploymentState, StoreError> {
        self.with_conn(|conn, _| deployment::state(conn, id).map_err(|e| e.into()))
            .await
    }

    pub(crate) async fn fail_subgraph(
        &self,
        id: DeploymentHash,
        error: SubgraphError,
    ) -> Result<(), StoreError> {
        self.with_conn(move |conn, _| {
            conn.transaction(|| deployment::fail(conn, &id, &error))
                .map_err(Into::into)
        })
        .await?;
        Ok(())
    }

    pub(crate) fn replica_for_query(
        &self,
        for_subscription: bool,
    ) -> Result<ReplicaId, StoreError> {
        use std::sync::atomic::Ordering;

        let replica_id = match for_subscription {
            // Pick a weighted ReplicaId. `replica_order` contains a list of
            // replicas with repetitions according to their weight
            false => {
                let weights_count = self.replica_order.len();
                let index =
                    self.conn_round_robin_counter.fetch_add(1, Ordering::SeqCst) % weights_count;
                *self.replica_order.get(index).unwrap()
            }
            // Subscriptions always go to the main replica.
            true => ReplicaId::Main,
        };

        Ok(replica_id)
    }

    pub(crate) async fn load_dynamic_data_sources(
        &self,
        site: Arc<Site>,
        block: BlockNumber,
        manifest_idx_and_name: Vec<(u32, String)>,
    ) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
        self.with_conn(move |conn, _| {
            conn.transaction(|| crate::dynds::load(conn, &site, block, manifest_idx_and_name))
                .map_err(Into::into)
        })
        .await
    }

    pub(crate) async fn causality_region_curr_val(
        &self,
        site: Arc<Site>,
    ) -> Result<Option<CausalityRegion>, StoreError> {
        self.with_conn(move |conn, _| {
            Ok(conn.transaction(|| crate::dynds::causality_region_curr_val(conn, &site))?)
        })
        .await
    }

    pub(crate) async fn exists_and_synced(&self, id: DeploymentHash) -> Result<bool, StoreError> {
        self.with_conn(move |conn, _| {
            conn.transaction(|| deployment::exists_and_synced(conn, &id))
                .map_err(Into::into)
        })
        .await
    }

    pub(crate) fn graft_pending(
        &self,
        id: &DeploymentHash,
    ) -> Result<Option<(DeploymentHash, BlockPtr)>, StoreError> {
        let conn = self.get_conn()?;
        deployment::graft_pending(&conn, id)
    }

    /// Bring the subgraph into a state where we can start or resume
    /// indexing.
    ///
    /// If `graft_src` is `Some(..)`, copy data from that subgraph. It
    /// should only be `Some(..)` if we know we still need to copy data. The
    /// code is idempotent so that a copy process that has been interrupted
    /// can be resumed seamlessly, but the code sets the block pointer back
    /// to the graph point, so that calling this needlessly with `Some(..)`
    /// will remove any progress that might have been made since the last
    /// time the deployment was started.
    pub(crate) fn start_subgraph(
        &self,
        logger: &Logger,
        site: Arc<Site>,
        graft_src: Option<(Arc<Layout>, BlockPtr, SubgraphDeploymentEntity)>,
    ) -> Result<(), StoreError> {
        let dst = self.find_layout(site.cheap_clone())?;

        // If `graft_src` is `Some`, then there is a pending graft.
        if let Some((src, block, src_deployment)) = graft_src {
            info!(
                logger,
                "Initializing graft by copying data from {} to {}",
                src.catalog.site.namespace,
                dst.catalog.site.namespace
            );

            let src_manifest_idx_and_name = src_deployment.manifest.template_idx_and_name()?;
            let dst_manifest_idx_and_name = self
                .load_deployment(dst.site.clone())?
                .manifest
                .template_idx_and_name()?;

            // Copy subgraph data
            // We allow both not copying tables at all from the source, as well
            // as adding new tables in `self`; we only need to check that tables
            // that actually need to be copied from the source are compatible
            // with the corresponding tables in `self`
            let copy_conn = crate::copy::Connection::new(
                logger,
                self.pool.clone(),
                src.clone(),
                dst.clone(),
                block.clone(),
                src_manifest_idx_and_name,
                dst_manifest_idx_and_name,
            )?;
            let status = copy_conn.copy_data()?;
            if status == crate::copy::Status::Cancelled {
                return Err(StoreError::Canceled);
            }

            let conn = self.get_conn()?;
            conn.transaction(|| -> Result<(), StoreError> {
                // Copy shared dynamic data sources and adjust their ID; if
                // the subgraph uses private data sources, that is done by
                // `copy::Connection::copy_data` since it requires access to
                // the source schema which in sharded setups is only
                // available while that function runs
                let start = Instant::now();
                let count = dynds::shared::copy(&conn, &src.site, &dst.site, block.number)?;
                info!(logger, "Copied {} dynamic data sources", count;
                      "time_ms" => start.elapsed().as_millis());

                // Copy errors across
                let start = Instant::now();
                let count = deployment::copy_errors(&conn, &src.site, &dst.site, &block)?;
                info!(logger, "Copied {} existing errors", count;
                      "time_ms" => start.elapsed().as_millis());

                catalog::copy_account_like(&conn, &src.site, &dst.site)?;

                // Rewind the subgraph so that entity versions that are
                // clamped in the future (beyond `block`) become valid for
                // all blocks after `block`. `revert_block` gets rid of
                // everything including the block passed to it. We want to
                // preserve `block` and therefore revert `block+1`
                let start = Instant::now();
                let block_to_revert: BlockNumber = block
                    .number
                    .checked_add(1)
                    .expect("block numbers fit into an i32");
                dst.revert_block(&conn, block_to_revert)?;
                info!(logger, "Rewound subgraph to block {}", block.number;
                      "time_ms" => start.elapsed().as_millis());

                let start = Instant::now();
                deployment::set_entity_count(&conn, &dst.site, &dst.count_query)?;
                info!(logger, "Counted the entities";
                      "time_ms" => start.elapsed().as_millis());

                deployment::set_history_blocks(
                    &conn,
                    &dst.site,
                    src_deployment.manifest.history_blocks,
                )?;

                // Analyze all tables for this deployment
                for entity_name in dst.tables.keys() {
                    self.analyze_with_conn(site.cheap_clone(), entity_name.as_str(), &conn)?;
                }

                // The `earliest_block` for `src` might have changed while
                // we did the copy if `src` was pruned while we copied;
                // adjusting it very late in the copy process ensures that
                // we truly do have all the data starting at
                // `earliest_block` and do not inadvertently expose data
                // that might be incomplete because a prune on the source
                // removed data just before we copied it
                deployment::copy_earliest_block(&conn, &src.site, &dst.site)?;

                // Set the block ptr to the graft point to signal that we successfully
                // performed the graft
                crate::deployment::forward_block_ptr(&conn, &dst.site.deployment, &block)?;
                info!(logger, "Subgraph successfully initialized";
                    "time_ms" => start.elapsed().as_millis());
                Ok(())
            })?;
        }
        // Make sure the block pointer is set. This is important for newly
        // deployed subgraphs so that we respect the 'startBlock' setting
        // the first time the subgraph is started
        let conn = self.get_conn()?;
        conn.transaction(|| crate::deployment::initialize_block_ptr(&conn, &dst.site))?;
        Ok(())
    }

    // If the current block of the deployment is the same as the fatal error,
    // we revert all block operations to it's parent/previous block.
    //
    // This should be called once per subgraph on `graph-node` initialization,
    // before processing the first block on start.
    //
    // It will do nothing (early return) if:
    //
    // - There's no fatal error for the subgraph
    // - The error is NOT deterministic
    pub(crate) fn unfail_deterministic_error(
        &self,
        site: Arc<Site>,
        current_ptr: &BlockPtr,
        parent_ptr: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        let conn = &self.get_conn()?;
        let deployment_id = &site.deployment;

        conn.transaction(|| {
            // We'll only unfail subgraphs that had fatal errors
            let subgraph_error = match ErrorDetail::fatal(conn, deployment_id)? {
                Some(fatal_error) => fatal_error,
                // If the subgraph is not failed then there is nothing to do.
                None => return Ok(UnfailOutcome::Noop),
            };

            // Confidence check
            if !subgraph_error.deterministic {
                return Ok(UnfailOutcome::Noop); // Nothing to do
            }

            use deployment::SubgraphHealth::*;
            // Decide status based on if there are any errors for the previous/parent block
            let prev_health =
                if deployment::has_deterministic_errors(conn, deployment_id, parent_ptr.number)? {
                    Unhealthy
                } else {
                    Healthy
                };

            match &subgraph_error.block_hash {
                // The error happened for the current deployment head.
                // We should revert everything (deployment head, subgraph errors, etc)
                // to the previous/parent hash/block.
                Some(bytes) if bytes == current_ptr.hash.as_slice() => {
                    info!(
                        self.logger,
                        "Reverting errored block";
                        "subgraph_id" => deployment_id,
                        "from_block_number" => format!("{}", current_ptr.number),
                        "from_block_hash" => format!("{}", current_ptr.hash),
                        "to_block_number" => format!("{}", parent_ptr.number),
                        "to_block_hash" => format!("{}", parent_ptr.hash),
                    );

                    // We ignore the StoreEvent that's being returned, we'll not use it.
                    //
                    // We reset the firehose cursor. That way, on resume, Firehose will start from
                    // the block_ptr instead (with sanity checks to ensure it's resuming at the
                    // correct block).
                    let _ = self.revert_block_operations(site.clone(), parent_ptr.clone(), &FirehoseCursor::None)?;

                    // Unfail the deployment.
                    deployment::update_deployment_status(conn, deployment_id, prev_health, None,None)?;

                    Ok(UnfailOutcome::Unfailed)
                }
                // Found error, but not for deployment head, we don't need to
                // revert the block operations.
                //
                // If you find this warning in the logs, something is wrong, this
                // shoudn't happen.
                Some(hash_bytes) => {
                    warn!(self.logger, "Subgraph error does not have same block hash as deployment head";
                        "subgraph_id" => deployment_id,
                        "error_id" => &subgraph_error.id,
                        "error_block_hash" => format!("0x{}", hex::encode(hash_bytes)),
                        "deployment_head" => format!("{}", current_ptr.hash),
                    );

                    Ok(UnfailOutcome::Noop)
                }
                // Same as branch above, if you find this warning in the logs,
                // something is wrong, this shouldn't happen.
                None => {
                    warn!(self.logger, "Subgraph error should have block hash";
                        "subgraph_id" => deployment_id,
                        "error_id" => &subgraph_error.id,
                    );

                    Ok(UnfailOutcome::Noop)
                }
            }
        })
    }

    // If a non-deterministic error happens and the deployment head advances,
    // we should unfail the subgraph (status: Healthy, failed: false) and delete
    // the error itself.
    //
    // This should be called after successfully processing a block for a subgraph.
    //
    // It will do nothing (early return) if:
    //
    // - There's no fatal error for the subgraph
    // - The error IS deterministic
    pub(crate) fn unfail_non_deterministic_error(
        &self,
        site: Arc<Site>,
        current_ptr: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        let conn = &self.get_conn()?;
        let deployment_id = &site.deployment;

        conn.transaction(|| {
            // We'll only unfail subgraphs that had fatal errors
            let subgraph_error = match ErrorDetail::fatal(conn, deployment_id)? {
                Some(fatal_error) => fatal_error,
                // If the subgraph is not failed then there is nothing to do.
                None => return Ok(UnfailOutcome::Noop),
            };

            // Confidence check
            if subgraph_error.deterministic {
                return Ok(UnfailOutcome::Noop); // Nothing to do
            }

            match subgraph_error.block_range {
                // Deployment head (current_ptr) advanced more than the error.
                // That means it's healthy, and the non-deterministic error got
                // solved (didn't happen on another try).
                (Bound::Included(error_block_number), _)
                    if current_ptr.number >= error_block_number =>
                    {
                        info!(
                            self.logger,
                            "Unfailing the deployment status";
                            "subgraph_id" => deployment_id,
                        );

                        // Unfail the deployment.
                        deployment::update_deployment_status(
                            conn,
                            deployment_id,
                            deployment::SubgraphHealth::Healthy,
                            None,
                            None,
                        )?;

                        // Delete the fatal error.
                        deployment::delete_error(conn, &subgraph_error.id)?;

                        Ok(UnfailOutcome::Unfailed)
                    }
                // NOOP, the deployment head is still before where non-deterministic error happened.
                block_range => {
                    info!(
                        self.logger,
                        "Subgraph error is still ahead of deployment head, nothing to unfail";
                        "subgraph_id" => deployment_id,
                        "block_number" => format!("{}", current_ptr.number),
                        "block_hash" => format!("{}", current_ptr.hash),
                        "error_block_range" => format!("{:?}", block_range),
                        "error_block_hash" => subgraph_error.block_hash.as_ref().map(|hash| format!("0x{}", hex::encode(hash))),
                    );

                    Ok(UnfailOutcome::Noop)
                }
            }
        })
    }

    #[cfg(debug_assertions)]
    pub fn error_count(&self, id: &DeploymentHash) -> Result<usize, StoreError> {
        let conn = self.get_conn()?;
        deployment::error_count(&conn, id)
    }

    pub(crate) async fn mirror_primary_tables(&self, logger: &Logger) {
        self.pool.mirror_primary_tables().await.unwrap_or_else(|e| {
            warn!(logger, "Mirroring primary tables failed. We will try again in a few minutes";
                  "error" => e.to_string(),
                  "shard" => self.pool.shard.as_str())
        });
    }

    pub(crate) async fn refresh_materialized_views(&self, logger: &Logger) {
        async fn run(store: &DeploymentStore) -> Result<(), StoreError> {
            // We hardcode our materialized views, but could also use
            // pg_matviews to list all of them, though that might inadvertently
            // refresh materialized views that operators created themselves
            const VIEWS: [&str; 3] = [
                "info.table_sizes",
                "info.subgraph_sizes",
                "info.chain_sizes",
            ];
            store
                .with_conn(|conn, cancel| {
                    for view in VIEWS {
                        let query = format!("refresh materialized view {}", view);
                        diesel::sql_query(&query).execute(conn)?;
                        cancel.check_cancel()?;
                    }
                    Ok(())
                })
                .await
        }

        run(self).await.unwrap_or_else(|e| {
            warn!(logger, "Refreshing materialized views failed. We will try again in a few hours";
                  "error" => e.to_string(),
                  "shard" => self.pool.shard.as_str())
        });
    }

    pub(crate) async fn health(
        &self,
        site: &Site,
    ) -> Result<deployment::SubgraphHealth, StoreError> {
        let id = site.id;
        self.with_conn(move |conn, _| deployment::health(conn, id).map_err(Into::into))
            .await
    }

    pub(crate) async fn set_manifest_raw_yaml(
        &self,
        site: Arc<Site>,
        raw_yaml: String,
    ) -> Result<(), StoreError> {
        self.with_conn(move |conn, _| {
            deployment::set_manifest_raw_yaml(conn, &site, &raw_yaml).map_err(Into::into)
        })
        .await
    }
}

/// Tries to fetch a [`Table`] either by its Entity name or its SQL name.
///
/// Since we allow our input to be either camel-case or snake-case, we must retry the
/// search using the latter if the search for the former fails.
fn resolve_table_name<'a>(layout: &'a Layout, name: &'_ str) -> Result<&'a Table, StoreError> {
    layout
        .input_schema
        .entity_type(name)
        .map_err(StoreError::from)
        .and_then(|et| layout.table_for_entity(&et))
        .map(Deref::deref)
        .or_else(|_error| {
            let sql_name = SqlName::from(name);
            layout
                .table(&sql_name)
                .ok_or_else(|| StoreError::UnknownTable(name.to_owned()))
        })
}

/// Resolves column names against the `table`. The `field_names` can be
/// either GraphQL attributes or the SQL names of columns. We also accept
/// the names `block_range` and `block$` and map that to the correct name
/// for the block range column for that table.
fn resolve_column_names_and_index_exprs<'a, T: AsRef<str>>(
    table: &'a Table,
    field_names: &[T],
) -> Result<(Vec<&'a SqlName>, Vec<String>), StoreError> {
    let mut column_names = Vec::new();
    let mut index_exprs = Vec::new();

    for field in field_names {
        let (column_name, index_expr) =
            if field.as_ref() == BLOCK_RANGE_COLUMN || field.as_ref() == BLOCK_COLUMN {
                let name = table.block_column();
                (name, name.to_string())
            } else {
                resolve_column(table, field.as_ref())?
            };

        column_names.push(column_name);
        index_exprs.push(index_expr);
    }

    Ok((column_names, index_exprs))
}

/// Resolves a column name against the `table`. The `field` can be
/// either GraphQL attribute or the SQL name of a column.
fn resolve_column<'a>(table: &'a Table, field: &str) -> Result<(&'a SqlName, String), StoreError> {
    table
        .column_for_field(field)
        .or_else(|_| {
            let sql_name = SqlName::from(field);
            table
                .column(&sql_name)
                .ok_or_else(|| StoreError::UnknownField(field.to_string()))
        })
        .map(|column| {
            let index_expr =
                Table::calculate_index_method_and_expression(table.immutable, column).1;
            (&column.name, index_expr)
        })
}

/// A helper to log progress during pruning that is kicked off from
/// `transact_block_operations`
struct OngoingPruneReporter {
    logger: Logger,
    start: Instant,
    analyze_start: Instant,
    analyze_duration: Duration,
    rows_copied: usize,
    rows_deleted: usize,
    tables: Vec<String>,
}

impl OngoingPruneReporter {
    fn new(logger: Logger) -> Box<Self> {
        Box::new(Self {
            logger,
            start: Instant::now(),
            analyze_start: Instant::now(),
            analyze_duration: Duration::from_secs(0),
            rows_copied: 0,
            rows_deleted: 0,
            tables: Vec::new(),
        })
    }
}

impl OngoingPruneReporter {
    fn tables_as_string(&self) -> String {
        if self.tables.is_empty() {
            "ø".to_string()
        } else {
            format!("[{}]", self.tables.iter().join(","))
        }
    }
}

impl PruneReporter for OngoingPruneReporter {
    fn start(&mut self, req: &PruneRequest) {
        self.start = Instant::now();
        info!(&self.logger, "Start pruning historical entities";
              "history_blocks" => req.history_blocks,
              "earliest_block" => req.earliest_block,
              "latest_block" => req.latest_block);
    }

    fn start_analyze(&mut self) {
        self.analyze_start = Instant::now()
    }

    fn finish_analyze(&mut self, _stats: &[VersionStats], analyzed: &[&str]) {
        self.analyze_duration += self.analyze_start.elapsed();
        debug!(&self.logger, "Analyzed {} tables", analyzed.len(); "time_s" => self.analyze_start.elapsed().as_secs());
    }

    fn start_table(&mut self, table: &str) {
        self.tables.push(table.to_string());
    }

    fn prune_batch(&mut self, _table: &str, rows: usize, phase: PrunePhase, _finished: bool) {
        match phase.strategy() {
            PruningStrategy::Rebuild => self.rows_copied += rows,
            PruningStrategy::Delete => self.rows_deleted += rows,
        }
    }
    fn finish(&mut self) {
        info!(
            &self.logger,
            "Finished pruning entities";
            "tables" => self.tables_as_string(),
            "rows_deleted" => self.rows_deleted,
            "rows_copied" => self.rows_copied,
            "time_s" => self.start.elapsed().as_secs(),
            "analyze_time_s" => self.analyze_duration.as_secs()
        )
    }
}
