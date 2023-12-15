use diesel::{
    pg::Pg,
    serialize::Output,
    sql_types::Text,
    types::{FromSql, ToSql},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{atomic::AtomicU8, Arc, Mutex},
};
use std::{fmt, io::Write};
use std::{iter::FromIterator, time::Duration};

use graph::{
    cheap_clone::CheapClone,
    components::{
        server::index_node::VersionInfo,
        store::{
            self, BlockPtrForNumber, BlockStore, DeploymentLocator, EnsLookup as EnsLookupTrait,
            PruneReporter, PruneRequest, SubgraphFork,
        },
    },
    constraint_violation,
    data::query::QueryTarget,
    data::subgraph::{schema::DeploymentCreate, status, DeploymentFeatures},
    prelude::{
        anyhow, futures03::future::join_all, lazy_static, o, web3::types::Address, ApiVersion,
        BlockNumber, BlockPtr, ChainStore, DeploymentHash, EntityOperation, Logger,
        MetricsRegistry, NodeId, PartialBlockPtr, StoreError, SubgraphDeploymentEntity,
        SubgraphName, SubgraphStore as SubgraphStoreTrait, SubgraphVersionSwitchingMode,
    },
    prelude::{CancelableError, StoreEvent},
    schema::{ApiSchema, InputSchema},
    url::Url,
    util::timed_cache::TimedCache,
};

use crate::{
    connection_pool::ConnectionPool,
    deployment::{OnSync, SubgraphHealth},
    primary,
    primary::{DeploymentId, Mirror as PrimaryMirror, Site},
    relational::{index::Method, Layout},
    writable::WritableStore,
    NotificationSender,
};
use crate::{
    deployment_store::{DeploymentStore, ReplicaId},
    detail::DeploymentDetail,
    primary::UnusedDeployment,
};
use crate::{fork, relational::index::CreateIndex, relational::SqlName};

/// The name of a database shard; valid names must match `[a-z0-9_]+`
#[derive(Clone, Debug, Eq, PartialEq, Hash, AsExpression, FromSqlRow)]
pub struct Shard(String);

lazy_static! {
    /// The name of the primary shard that contains all instance-wide data
    pub static ref PRIMARY_SHARD: Shard = Shard("primary".to_string());
}

/// How long to cache information about a deployment site
const SITES_CACHE_TTL: Duration = Duration::from_secs(120);

impl Shard {
    pub fn new(name: String) -> Result<Self, StoreError> {
        if name.is_empty() {
            return Err(StoreError::InvalidIdentifier(
                "shard names must not be empty".to_string(),
            ));
        }
        if name.len() > 30 {
            return Err(StoreError::InvalidIdentifier(format!(
                "shard names can be at most 30 characters, but `{}` has {} characters",
                name,
                name.len()
            )));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(StoreError::InvalidIdentifier(format!(
                "shard name `{}` is invalid: shard names must only contain lowercase alphanumeric characters or '_'", name
            )));
        }
        Ok(Shard(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Shard {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromSql<Text, Pg> for Shard {
    fn from_sql(bytes: Option<&[u8]>) -> diesel::deserialize::Result<Self> {
        let s = <String as FromSql<Text, Pg>>::from_sql(bytes)?;
        Shard::new(s).map_err(Into::into)
    }
}

impl ToSql<Text, Pg> for Shard {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> diesel::serialize::Result {
        <String as ToSql<Text, Pg>>::to_sql(&self.0, out)
    }
}

/// Decide where a new deployment should be placed based on the subgraph
/// name and the network it is indexing. If the deployment can be placed,
/// returns a list of eligible database shards for the deployment and the
/// names of the indexers that should index it. The deployment should then
/// be assigned to one of the returned indexers and placed into one of the
/// shards.
pub trait DeploymentPlacer {
    fn place(&self, name: &str, network: &str)
        -> Result<Option<(Vec<Shard>, Vec<NodeId>)>, String>;
}

/// Tools for managing unused deployments
pub mod unused {
    use graph::prelude::chrono::Duration;

    pub enum Filter {
        /// List all unused deployments
        All,
        /// List only deployments that are unused but have not been removed yet
        New,
        /// List only deployments that were recorded as unused at least this
        /// long ago but have not been removed at
        UnusedLongerThan(Duration),
    }
}

/// Multiplex store operations on subgraphs and deployments between a
/// primary and any number of additional storage shards. The primary
/// contains information about named subgraphs, and how the underlying
/// deployments are spread across shards, while the actual deployment data
/// and metadata is stored in the shards.  Depending on the configuration,
/// the database for the primary and for the shards can be the same
/// database, in which case they are all backed by one connection pool, or
/// separate databases in the same Postgres cluster, or entirely separate
/// clusters. Details of how to configure shards can be found in [this
/// document](https://github.com/graphprotocol/graph-node/blob/master/docs/config.md)
///
/// The primary uses the following database tables:
/// - `public.deployment_schemas`: immutable data about deployments,
///   including the shard that stores the deployment data and metadata, the
///   namespace in the shard that contains the deployment data, and the
///   network/chain that the  deployment is indexing
/// - `subgraphs.subgraph` and `subgraphs.subgraph_version`: information
///   about named subgraphs and how they map to deployments
/// - `subgraphs.subgraph_deployment_assignment`: which index node is
///   indexing what deployment
///
/// The primary is also the database that is used to send and receive
/// notifications through Postgres' `LISTEN`/`NOTIFY` mechanism. That is
/// used to send notifications about new blocks that a block ingestor has
/// discovered, and to send `StoreEvents`, which are used to broadcast
/// changes in deployment assignments and changes in subgraph data to
/// trigger updates on GraphQL subscriptions.
///
/// For each deployment, the corresponding shard contains a namespace for
/// the deployment data; the schema in that namespace is generated from the
/// deployment's GraphQL schema by the [crate::relational::Layout], which is
/// also responsible for modifying and querying subgraph data. Deployment
/// metadata is stored in tables in the `subgraphs` namespace in the same
/// shard as the deployment data. The most important of these tables are
///
/// - `subgraphs.subgraph_deployment`: the main table for deployment
///   metadata; most importantly, it stores the pointer to the current
///   subgraph head, i.e., the block up to which the subgraph has indexed
///   the chain, together with other things like whether the subgraph has
///   synced, whether it has failed and whether it encountered any errors
/// - `subgraphs.subgraph_manifest`: immutable information derived from the
///   YAML manifest for the deployment
/// - `subgraphs.dynamic_ethereum_contract_data_source`: the data sources
///   that the subgraph has created from templates in the manifest.
/// - `subgraphs.subgraph_error`: details about errors that the deployment
///   has encountered
///
/// The `SubgraphStore` mostly orchestrates access to the primary and the
/// shards.  The actual work is done by code in the `primary` module for
/// queries against the primary store, and by the `DeploymentStore` for
/// access to deployment data and metadata.
#[derive(Clone)]
pub struct SubgraphStore {
    inner: Arc<SubgraphStoreInner>,
    /// Base URL for the GraphQL endpoint from which
    /// subgraph forks will fetch entities.
    /// Example: https://api.thegraph.com/subgraphs/
    fork_base: Option<Url>,
}

impl SubgraphStore {
    /// Create a new store for subgraphs that distributes deployments across
    /// multiple databases
    ///
    /// `stores` is a list of the shards. The tuple contains the shard name, the main
    /// connection pool for the database, a list of read-only connections
    /// for the same database, and a list of weights determining how often
    /// to use the main pool and the read replicas for queries. The list
    /// of weights must be one longer than the list of read replicas, and
    /// `weights[0]` is used for the main pool.
    ///
    /// All write operations for a shard are performed against the main
    /// pool. One of the shards must be named `primary`
    ///
    /// The `placer` determines where `create_subgraph_deployment` puts a new deployment
    pub fn new(
        logger: &Logger,
        stores: Vec<(Shard, ConnectionPool, Vec<ConnectionPool>, Vec<usize>)>,
        placer: Arc<dyn DeploymentPlacer + Send + Sync + 'static>,
        sender: Arc<NotificationSender>,
        fork_base: Option<Url>,
        registry: Arc<MetricsRegistry>,
    ) -> Self {
        Self {
            inner: Arc::new(SubgraphStoreInner::new(
                logger, stores, placer, sender, registry,
            )),
            fork_base,
        }
    }

    pub async fn get_proof_of_indexing(
        &self,
        id: &DeploymentHash,
        indexer: &Option<Address>,
        block: BlockPtr,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        self.inner.get_proof_of_indexing(id, indexer, block).await
    }

    pub(crate) async fn get_public_proof_of_indexing(
        &self,
        id: &DeploymentHash,
        block_number: BlockNumber,
        block_store: Arc<impl BlockStore>,
        fetch_block_ptr: &dyn BlockPtrForNumber,
    ) -> Result<Option<(PartialBlockPtr, [u8; 32])>, StoreError> {
        self.inner
            .get_public_proof_of_indexing(id, block_number, block_store, fetch_block_ptr)
            .await
    }

    pub fn notification_sender(&self) -> Arc<NotificationSender> {
        self.sender.clone()
    }
}

impl std::ops::Deref for SubgraphStore {
    type Target = SubgraphStoreInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct SubgraphStoreInner {
    mirror: PrimaryMirror,
    stores: HashMap<Shard, Arc<DeploymentStore>>,
    /// Cache for the mapping from deployment id to shard/namespace/id. Only
    /// active sites are cached here to ensure we have a unique mapping from
    /// `SubgraphDeploymentId` to `Site`. The cache keeps entry only for
    /// `SITES_CACHE_TTL` so that changes, in particular, activation of a
    /// different deployment for the same hash propagate across different
    /// graph-node processes over time.
    sites: TimedCache<DeploymentHash, Site>,
    placer: Arc<dyn DeploymentPlacer + Send + Sync + 'static>,
    sender: Arc<NotificationSender>,
    writables: Mutex<HashMap<DeploymentId, Arc<WritableStore>>>,
    registry: Arc<MetricsRegistry>,
}

impl SubgraphStoreInner {
    /// Create a new store for subgraphs that distributes deployments across
    /// multiple databases
    ///
    /// `stores` is a list of the shards. The tuple contains the shard name, the main
    /// connection pool for the database, a list of read-only connections
    /// for the same database, and a list of weights determining how often
    /// to use the main pool and the read replicas for queries. The list
    /// of weights must be one longer than the list of read replicas, and
    /// `weights[0]` is used for the main pool.
    ///
    /// All write operations for a shard are performed against the main
    /// pool. One of the shards must be named `primary`
    ///
    /// The `placer` determines where `create_subgraph_deployment` puts a new deployment
    pub fn new(
        logger: &Logger,
        stores: Vec<(Shard, ConnectionPool, Vec<ConnectionPool>, Vec<usize>)>,
        placer: Arc<dyn DeploymentPlacer + Send + Sync + 'static>,
        sender: Arc<NotificationSender>,
        registry: Arc<MetricsRegistry>,
    ) -> Self {
        let mirror = {
            let pools = HashMap::from_iter(
                stores
                    .iter()
                    .map(|(name, pool, _, _)| (name.clone(), pool.clone())),
            );
            PrimaryMirror::new(&pools)
        };
        let stores = HashMap::from_iter(stores.into_iter().map(
            |(name, main_pool, read_only_pools, weights)| {
                let logger = logger.new(o!("shard" => name.to_string()));

                (
                    name,
                    Arc::new(DeploymentStore::new(
                        &logger,
                        main_pool,
                        read_only_pools,
                        weights,
                    )),
                )
            },
        ));
        let sites = TimedCache::new(SITES_CACHE_TTL);
        SubgraphStoreInner {
            mirror,
            stores,
            sites,
            placer,
            sender,
            writables: Mutex::new(HashMap::new()),
            registry,
        }
    }

    // Only needed for tests
    #[cfg(debug_assertions)]
    pub(crate) fn clear_caches(&self) {
        for store in self.stores.values() {
            store.layout_cache.clear();
        }
        self.sites.clear();
    }

    // Only needed for tests
    #[cfg(debug_assertions)]
    pub fn shard(&self, deployment: &DeploymentLocator) -> Result<Shard, StoreError> {
        self.find_site(deployment.id.into())
            .map(|site| site.shard.clone())
    }

    fn cache_active(&self, site: &Arc<Site>) {
        if site.active {
            self.sites.set(site.deployment.clone(), site.clone());
        }
    }

    /// Return the active `Site` for this deployment hash
    fn site(&self, id: &DeploymentHash) -> Result<Arc<Site>, StoreError> {
        if let Some(site) = self.sites.get(id) {
            return Ok(site);
        }

        let site = self
            .mirror
            .find_active_site(id)?
            .ok_or_else(|| StoreError::DeploymentNotFound(id.to_string()))?;
        let site = Arc::new(site);

        self.cache_active(&site);
        Ok(site)
    }

    fn evict(&self, id: &DeploymentHash) -> Result<(), StoreError> {
        if let Some((site, _)) = self.sites.remove(id) {
            let store = self.stores.get(&site.shard).ok_or_else(|| {
                constraint_violation!(
                    "shard {} for deployment sgd{} not found when evicting",
                    site.shard,
                    site.id
                )
            })?;
            store.layout_cache.remove(&site);
        }
        Ok(())
    }

    pub(crate) fn find_site(&self, id: DeploymentId) -> Result<Arc<Site>, StoreError> {
        if let Some(site) = self.sites.find(|site| site.id == id) {
            return Ok(site);
        }

        let site = self
            .mirror
            .find_site_by_ref(id)?
            .ok_or_else(|| StoreError::DeploymentNotFound(id.to_string()))?;
        let site = Arc::new(site);

        self.cache_active(&site);
        Ok(site)
    }

    /// Return the store and site for the active deployment of this
    /// deployment hash
    fn store(&self, id: &DeploymentHash) -> Result<(&Arc<DeploymentStore>, Arc<Site>), StoreError> {
        let site = self.site(id)?;
        let store = self
            .stores
            .get(&site.shard)
            .ok_or_else(|| StoreError::UnknownShard(site.shard.to_string()))?;
        Ok((store, site))
    }

    pub(crate) fn for_site(&self, site: &Site) -> Result<&Arc<DeploymentStore>, StoreError> {
        self.stores
            .get(&site.shard)
            .ok_or_else(|| StoreError::UnknownShard(site.shard.to_string()))
    }

    pub(crate) fn layout(&self, id: &DeploymentHash) -> Result<Arc<Layout>, StoreError> {
        let (store, site) = self.store(id)?;
        store.find_layout(site)
    }

    fn place_on_node(
        &self,
        mut nodes: Vec<NodeId>,
        default_node: NodeId,
    ) -> Result<NodeId, StoreError> {
        match nodes.len() {
            0 => {
                // This is really a configuration error
                Ok(default_node)
            }
            1 => Ok(nodes.pop().unwrap()),
            _ => {
                let conn = self.primary_conn()?;

                // unwrap is fine since nodes is not empty
                let node = conn.least_assigned_node(&nodes)?.unwrap();
                Ok(node)
            }
        }
    }

    fn place_in_shard(&self, mut shards: Vec<Shard>) -> Result<Shard, StoreError> {
        match shards.len() {
            0 => Ok(PRIMARY_SHARD.clone()),
            1 => Ok(shards.pop().unwrap()),
            _ => {
                let conn = self.primary_conn()?;

                // unwrap is fine since shards is not empty
                let shard = conn.least_used_shard(&shards)?.unwrap();
                Ok(shard)
            }
        }
    }

    fn place(
        &self,
        name: &SubgraphName,
        network_name: &str,
        default_node: NodeId,
    ) -> Result<(Shard, NodeId), StoreError> {
        // We try to place the deployment according to the configured rules.
        // If they don't yield a match, place into the primary and have
        // `default_node` index the deployment. The latter can only happen
        // when `graph-node` is not using a configuration file, but
        // uses the legacy command-line options as configuration
        let placement = self
            .placer
            .place(name.as_str(), network_name)
            .map_err(|msg| {
                constraint_violation!("illegal indexer name in deployment rule: {}", msg)
            })?;

        match placement {
            None => Ok((PRIMARY_SHARD.clone(), default_node)),
            Some((shards, nodes)) => {
                let node = self.place_on_node(nodes, default_node)?;
                let shard = self.place_in_shard(shards)?;

                Ok((shard, node))
            }
        }
    }

    /// Create a new deployment. This requires creating an entry in
    /// `deployment_schemas` in the primary, the subgraph schema in another
    /// shard, assigning the deployment to a node, and handling any changes
    /// to current/pending versions of the subgraph `name`
    ///
    /// This process needs to modify two databases: the primary and the
    /// shard for the subgraph and is therefore not transactional. The code
    /// is careful to make sure this process is at least idempotent, so that
    /// a failed deployment creation operation can be fixed by deploying
    /// again.
    fn create_deployment_internal(
        &self,
        name: SubgraphName,
        schema: &InputSchema,
        deployment: DeploymentCreate,
        node_id: NodeId,
        network_name: String,
        mode: SubgraphVersionSwitchingMode,
        // replace == true is only used in tests; for non-test code, it must
        // be 'false'
        replace: bool,
    ) -> Result<DeploymentLocator, StoreError> {
        #[cfg(not(debug_assertions))]
        assert!(!replace);

        self.evict(schema.id())?;
        let graft_base = deployment.graft_base.as_ref();

        let (site, exists, node_id) = {
            // We need to deal with two situations:
            //   (1) We are really creating a new subgraph; it therefore needs
            //       to go in the shard and onto the node that the placement
            //       rules dictate
            //   (2) The deployment has previously been created, and either
            //       failed partway through, or the deployment rules have
            //       changed since the last time we created the deployment.
            //       In that case, we need to use the shard and node
            //       assignment that we used last time to avoid creating
            //       the same deployment in another shard
            let (shard, node_id) = self.place(&name, &network_name, node_id)?;
            let conn = self.primary_conn()?;
            let (site, site_was_created) =
                conn.allocate_site(shard, schema.id(), network_name, graft_base)?;
            let node_id = conn.assigned_node(&site)?.unwrap_or(node_id);
            (site, !site_was_created, node_id)
        };
        let site = Arc::new(site);

        // if the deployment already exists, we don't need to perform any copying
        // so we can set graft_base to None
        // if it doesn't exist, we need to copy the graft base to the new deployment
        let graft_base = if !exists {
            let graft_base = deployment
                .graft_base
                .as_ref()
                .map(|base| self.layout(base))
                .transpose()?;

            if let Some(graft_base) = &graft_base {
                self.primary_conn()?
                    .record_active_copy(graft_base.site.as_ref(), site.as_ref())?;
            }
            graft_base
        } else {
            None
        };

        // Create the actual databases schema and metadata entries
        let deployment_store = self
            .stores
            .get(&site.shard)
            .ok_or_else(|| StoreError::UnknownShard(site.shard.to_string()))?;
        deployment_store.create_deployment(
            schema,
            deployment,
            site.clone(),
            graft_base,
            replace,
            OnSync::None,
        )?;

        let exists_and_synced = |id: &DeploymentHash| {
            let (store, _) = self.store(id)?;
            store.deployment_exists_and_synced(id)
        };

        // FIXME: This simultaneously holds a `primary_conn` and a shard connection, which can
        // potentially deadlock.
        let pconn = self.primary_conn()?;
        pconn.transaction(|| -> Result<_, StoreError> {
            // Create subgraph, subgraph version, and assignment
            let changes =
                pconn.create_subgraph_version(name, &site, node_id, mode, exists_and_synced)?;

            let event = StoreEvent::new(changes);
            pconn.send_store_event(&self.sender, &event)?;
            Ok(())
        })?;
        Ok(site.as_ref().into())
    }

    pub fn copy_deployment(
        &self,
        src: &DeploymentLocator,
        shard: Shard,
        node: NodeId,
        block: BlockPtr,
        on_sync: OnSync,
    ) -> Result<DeploymentLocator, StoreError> {
        let src = self.find_site(src.id.into())?;
        let src_store = self.for_site(src.as_ref())?;
        let src_info = src_store.subgraph_info(src.as_ref())?;
        let src_loc = DeploymentLocator::from(src.as_ref());

        let dst = Arc::new(self.primary_conn()?.copy_site(&src, shard.clone())?);
        let dst_loc = DeploymentLocator::from(dst.as_ref());

        if src.id == dst.id {
            return Err(StoreError::Unknown(anyhow!(
                "can not copy deployment {} onto itself",
                src_loc
            )));
        }
        // The very last thing we do when we set up a copy here is assign it
        // to a node. Therefore, if `dst` is already assigned, this function
        // should not have been called.
        if let Some(node) = self.mirror.assigned_node(dst.as_ref())? {
            return Err(StoreError::Unknown(anyhow!(
                "can not copy into deployment {} since it is already assigned to node `{}`",
                dst_loc,
                node
            )));
        }
        let deployment = src_store.load_deployment(src.clone())?;
        if deployment.failed {
            return Err(StoreError::Unknown(anyhow!(
                "can not copy deployment {} because it has failed",
                src_loc
            )));
        }

        let history_blocks = deployment.manifest.history_blocks;

        // Transmogrify the deployment into a new one
        let deployment = DeploymentCreate {
            manifest: deployment.manifest,
            start_block: deployment.start_block.clone(),
            graft_base: Some(src.deployment.clone()),
            graft_block: Some(block),
            debug_fork: deployment.debug_fork,
            history_blocks: Some(history_blocks),
        };

        let graft_base = self.layout(&src.deployment)?;

        self.primary_conn()?
            .record_active_copy(src.as_ref(), dst.as_ref())?;

        // Create the actual databases schema and metadata entries
        let deployment_store = self
            .stores
            .get(&shard)
            .ok_or_else(|| StoreError::UnknownShard(shard.to_string()))?;

        deployment_store.create_deployment(
            &src_info.input,
            deployment,
            dst.clone(),
            Some(graft_base),
            false,
            on_sync,
        )?;

        let pconn = self.primary_conn()?;
        pconn.transaction(|| -> Result<_, StoreError> {
            // Create subgraph, subgraph version, and assignment. We use the
            // existence of an assignment as a signal that we already set up
            // the copy
            let changes = pconn.assign_subgraph(dst.as_ref(), &node)?;
            let event = StoreEvent::new(changes);
            pconn.send_store_event(&self.sender, &event)?;
            Ok(())
        })?;
        Ok(dst.as_ref().into())
    }

    /// Mark `deployment` as the only active deployment amongst all sites
    /// with the same deployment hash. Activating this specific deployment
    /// will make queries use that instead of whatever was active before
    pub fn activate(&self, deployment: &DeploymentLocator) -> Result<(), StoreError> {
        self.primary_conn()?.activate(deployment)?;
        // As a side-effect, this will update the `self.sites` cache with
        // the new active site
        self.find_site(deployment.id.into())?;
        Ok(())
    }

    // Only for tests to simplify their handling of test fixtures, so that
    // tests can reset the block pointer of a subgraph by recreating it
    #[cfg(debug_assertions)]
    pub fn create_deployment_replace(
        &self,
        name: SubgraphName,
        schema: &InputSchema,
        deployment: DeploymentCreate,
        node_id: NodeId,
        network_name: String,
        mode: SubgraphVersionSwitchingMode,
    ) -> Result<DeploymentLocator, StoreError> {
        self.create_deployment_internal(name, schema, deployment, node_id, network_name, mode, true)
    }

    pub(crate) fn send_store_event(&self, event: &StoreEvent) -> Result<(), StoreError> {
        let conn = self.primary_conn()?;
        conn.send_store_event(&self.sender, event)
    }

    /// Get a connection to the primary shard. Code must never hold one of these
    /// connections while also accessing a `DeploymentStore`, since both
    /// might draw connections from the same pool, and trying to get two
    /// connections can deadlock the entire process if the pool runs out
    /// of connections in between getting the first one and trying to get the
    /// second one.
    pub(crate) fn primary_conn(&self) -> Result<primary::Connection, StoreError> {
        let conn = self.mirror.primary().get()?;
        Ok(primary::Connection::new(conn))
    }

    pub(crate) async fn with_primary_conn<T: Send + 'static>(
        &self,
        f: impl 'static + Send + FnOnce(primary::Connection) -> Result<T, CancelableError<StoreError>>,
    ) -> Result<T, StoreError> {
        let pool = self.mirror.primary();
        pool.with_conn(move |pg_conn, _| {
            let conn = primary::Connection::new(pg_conn);
            f(conn)
        })
        .await
    }

    pub(crate) fn replica_for_query(
        &self,
        target: QueryTarget,
        for_subscription: bool,
    ) -> Result<(Arc<DeploymentStore>, Arc<Site>, ReplicaId), StoreError> {
        let id = match target {
            QueryTarget::Name(name, _) => self.mirror.current_deployment_for_subgraph(&name)?,
            QueryTarget::Deployment(id, _) => id,
        };

        let (store, site) = self.store(&id)?;
        let replica = store.replica_for_query(for_subscription)?;

        Ok((store.clone(), site, replica))
    }

    /// Delete all entities. This function exists solely for integration tests
    /// and should never be called from any other code. Unfortunately, Rust makes
    /// it very hard to export items just for testing
    #[cfg(debug_assertions)]
    pub fn delete_all_entities_for_test_use_only(&self) -> Result<(), StoreError> {
        let pconn = self.primary_conn()?;
        let schemas = pconn.sites()?;

        // Delete all subgraph schemas
        for schema in schemas {
            let (store, _) = self.store(&schema.deployment)?;
            store.drop_deployment_schema(&schema.namespace)?;
        }

        for store in self.stores.values() {
            store.drop_all_metadata()?;
        }
        self.clear_caches();
        Ok(())
    }

    /// Partition the list of deployments by the shard they belong to. As a
    /// side-effect, add all `sites` to the cache
    fn deployments_by_shard(
        &self,
        sites: Vec<Site>,
    ) -> Result<HashMap<Shard, Vec<Arc<Site>>>, StoreError> {
        let sites: Vec<_> = sites.into_iter().map(Arc::new).collect();
        for site in &sites {
            self.cache_active(site);
        }

        // Partition the list of deployments by shard
        let by_shard: HashMap<Shard, Vec<Arc<Site>>> =
            sites.into_iter().fold(HashMap::new(), |mut map, site| {
                map.entry(site.shard.clone()).or_default().push(site);
                map
            });
        Ok(by_shard)
    }

    /// Look for new unused deployments and add them to the `unused_deployments`
    /// table
    pub fn record_unused_deployments(&self) -> Result<Vec<DeploymentDetail>, StoreError> {
        let deployments = self.primary_conn()?.detect_unused_deployments()?;

        // deployments_by_shard takes an empty vec to mean 'give me everything',
        // so we short-circuit that here
        if deployments.is_empty() {
            return Ok(vec![]);
        }

        let by_shard = self.deployments_by_shard(deployments)?;
        // Go shard-by-shard to look up deployment statuses
        let mut details = Vec::new();
        for (shard, ids) in by_shard.into_iter() {
            let store = self
                .stores
                .get(&shard)
                .ok_or_else(|| StoreError::UnknownShard(shard.to_string()))?;
            let ids = ids
                .into_iter()
                .map(|site| site.deployment.to_string())
                .collect();
            details.extend(store.deployment_details(ids)?);
        }

        self.primary_conn()?.update_unused_deployments(&details)?;
        Ok(details)
    }

    pub fn list_unused_deployments(
        &self,
        filter: unused::Filter,
    ) -> Result<Vec<UnusedDeployment>, StoreError> {
        self.primary_conn()?.list_unused_deployments(filter)
    }

    /// Remove a deployment, i.e., all its data and metadata. This is only permissible
    /// if the deployment is unused in the sense that it is neither the current nor
    /// pending version of any subgraph, and is not currently assigned to any node
    pub fn remove_deployment(&self, id: DeploymentId) -> Result<(), StoreError> {
        let site = self.find_site(id)?;
        let store = self.for_site(site.as_ref())?;

        // Check that deployment is not assigned
        let mut removable = self.mirror.assigned_node(site.as_ref())?.is_none();

        // Check that it is not current/pending for any subgraph if it is
        // the active deployment of that subgraph
        if site.active
            && !self
                .primary_conn()?
                .subgraphs_using_deployment(site.as_ref())?
                .is_empty()
        {
            removable = false;
        }

        if removable {
            store.drop_deployment(&site)?;

            self.primary_conn()?.drop_site(site.as_ref())?;
        } else {
            self.primary_conn()?
                .unused_deployment_is_used(site.as_ref())?;
        }

        Ok(())
    }

    pub fn status_for_id(&self, id: graph::components::store::DeploymentId) -> status::Info {
        let filter = status::Filter::DeploymentIds(vec![id]);
        self.status(filter).unwrap().into_iter().next().unwrap()
    }

    pub(crate) fn status(&self, filter: status::Filter) -> Result<Vec<status::Info>, StoreError> {
        let sites = match filter {
            status::Filter::SubgraphName(name) => {
                let deployments = self.mirror.deployments_for_subgraph(&name)?;
                if deployments.is_empty() {
                    return Ok(Vec::new());
                }
                deployments
            }
            status::Filter::SubgraphVersion(name, use_current) => {
                let deployment = self.mirror.subgraph_version(&name, use_current)?;
                match deployment {
                    Some(deployment) => vec![deployment],
                    None => {
                        return Ok(Vec::new());
                    }
                }
            }
            status::Filter::Deployments(deployments) => {
                self.mirror.find_sites(&deployments, true)?
            }
            status::Filter::DeploymentIds(ids) => {
                let ids: Vec<_> = ids.into_iter().map(|id| id.into()).collect();
                self.mirror.find_sites_by_id(&ids)?
            }
        };

        let by_shard: HashMap<Shard, Vec<Arc<Site>>> = self.deployments_by_shard(sites)?;

        // Go shard-by-shard to look up deployment statuses
        let mut infos = Vec::new();
        for (shard, sites) in by_shard.into_iter() {
            let store = self
                .stores
                .get(&shard)
                .ok_or_else(|| StoreError::UnknownShard(shard.to_string()))?;
            infos.extend(store.deployment_statuses(&sites)?);
        }
        self.mirror.fill_assignments(&mut infos)?;
        Ok(infos)
    }

    pub(crate) fn version_info(&self, version: &str) -> Result<VersionInfo, StoreError> {
        if let Some((deployment_id, created_at)) = self.mirror.version_info(version)? {
            let id = DeploymentHash::new(deployment_id.clone())
                .map_err(|id| constraint_violation!("illegal deployment id {}", id))?;
            let (store, site) = self.store(&id)?;
            let statuses = store.deployment_statuses(&[site.clone()])?;
            let status = statuses
                .first()
                .ok_or_else(|| StoreError::DeploymentNotFound(deployment_id.clone()))?;
            let chain = status
                .chains
                .first()
                .ok_or_else(|| constraint_violation!("no chain info for {}", deployment_id))?;
            let latest_ethereum_block_number =
                chain.latest_block.as_ref().map(|block| block.number());
            let subgraph_info = store.subgraph_info(site.as_ref())?;
            let network = site.network.clone();

            let info = VersionInfo {
                created_at,
                deployment_id,
                latest_ethereum_block_number,
                total_ethereum_blocks_count: None,
                synced: status.synced,
                failed: status.health.is_failed(),
                description: subgraph_info.description,
                repository: subgraph_info.repository,
                schema: subgraph_info.input,
                network,
            };
            Ok(info)
        } else {
            Err(StoreError::DeploymentNotFound(version.to_string()))
        }
    }

    pub(crate) fn versions_for_subgraph_id(
        &self,
        subgraph_id: &str,
    ) -> Result<(Option<String>, Option<String>), StoreError> {
        self.mirror.versions_for_subgraph_id(subgraph_id)
    }

    pub(crate) fn subgraphs_for_deployment_hash(
        &self,
        deployment_hash: &str,
    ) -> Result<Vec<(String, String)>, StoreError> {
        self.mirror.subgraphs_by_deployment_hash(deployment_hash)
    }

    #[cfg(debug_assertions)]
    pub fn error_count(&self, id: &DeploymentHash) -> Result<usize, StoreError> {
        let (store, _) = self.store(id)?;
        store.error_count(id)
    }

    /// Vacuum the `subgraph_deployment` table in each shard
    pub(crate) async fn vacuum(&self) -> Vec<Result<(), StoreError>> {
        join_all(self.stores.values().map(|store| store.vacuum())).await
    }

    pub fn rewind(&self, id: DeploymentHash, block_ptr_to: BlockPtr) -> Result<(), StoreError> {
        let (store, site) = self.store(&id)?;
        let event = store.rewind(site, block_ptr_to)?;
        self.send_store_event(&event)
    }

    pub fn truncate(&self, id: DeploymentHash, block_ptr_to: BlockPtr) -> Result<(), StoreError> {
        let (store, site) = self.store(&id)?;
        let event = store.truncate(site, block_ptr_to)?;
        self.send_store_event(&event)
    }

    pub(crate) async fn get_proof_of_indexing(
        &self,
        id: &DeploymentHash,
        indexer: &Option<Address>,
        block: BlockPtr,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        let (store, site) = self.store(id)?;
        store.get_proof_of_indexing(site, indexer, block).await
    }

    pub(crate) async fn get_public_proof_of_indexing(
        &self,
        id: &DeploymentHash,
        block_number: BlockNumber,
        block_store: Arc<impl BlockStore>,
        fetch_block_ptr: &dyn BlockPtrForNumber,
    ) -> Result<Option<(PartialBlockPtr, [u8; 32])>, StoreError> {
        let (store, site) = self.store(id)?;

        let block_hash = {
            let chain_store = match block_store.chain_store(&site.network) {
                Some(chain_store) => chain_store,
                None => return Ok(None),
            };
            let mut hashes = chain_store.block_hashes_by_block_number(block_number)?;

            // If we have multiple versions of this block using any of them could introduce
            // non-determinism because we don't know which one is the right one
            if hashes.len() == 1 {
                hashes.pop().unwrap()
            } else {
                match fetch_block_ptr
                    .block_ptr_for_number(site.network.clone(), block_number)
                    .await
                    .ok()
                    .flatten()
                {
                    None => return Ok(None),
                    Some(block_ptr) => block_ptr.hash,
                }
            }
        };

        let block_for_poi_query = BlockPtr::new(block_hash.clone(), block_number);
        let indexer = Some(Address::zero());
        let poi = store
            .get_proof_of_indexing(site, &indexer, block_for_poi_query)
            .await?;

        Ok(poi.map(|poi| {
            (
                PartialBlockPtr {
                    number: block_number,
                    hash: Some(block_hash),
                },
                poi,
            )
        }))
    }

    // Only used by tests
    #[cfg(debug_assertions)]
    pub fn find(
        &self,
        query: graph::prelude::EntityQuery,
    ) -> Result<Vec<graph::prelude::Entity>, graph::prelude::QueryExecutionError> {
        let (store, site) = self.store(&query.subgraph_id)?;
        store.find(site, query)
    }

    pub fn locate_in_shard(
        &self,
        hash: &DeploymentHash,
        shard: Shard,
    ) -> Result<Option<DeploymentLocator>, StoreError> {
        Ok(self
            .mirror
            .find_site_in_shard(hash, &shard)?
            .as_ref()
            .map(|site| site.into()))
    }

    pub async fn mirror_primary_tables(&self, logger: &Logger) {
        join_all(
            self.stores
                .values()
                .map(|store| store.mirror_primary_tables(logger)),
        )
        .await;
    }

    pub async fn refresh_materialized_views(&self, logger: &Logger) {
        join_all(
            self.stores
                .values()
                .map(|store| store.refresh_materialized_views(logger)),
        )
        .await;
    }

    pub fn analyze(
        &self,
        deployment: &DeploymentLocator,
        entity_name: Option<&str>,
    ) -> Result<(), StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store.analyze(site, entity_name)
    }

    /// Return the statistics targets for all tables of `deployment`. The
    /// first return value is the default target, and the second value maps
    /// the name of each table to a map of column name to its statistics
    /// target. A value of `-1` means that the global default will be used.
    pub fn stats_targets(
        &self,
        deployment: &DeploymentLocator,
    ) -> Result<(i32, BTreeMap<SqlName, BTreeMap<SqlName, i32>>), StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store.stats_targets(site)
    }

    /// Set the statistics target for columns `columns` in `deployment`. If
    /// `entity` is `Some`, only set it for the table for that entity, if it
    /// is `None`, set it for all tables in the deployment.
    pub fn set_stats_target(
        &self,
        deployment: &DeploymentLocator,
        entity: Option<&str>,
        columns: Vec<String>,
        target: i32,
    ) -> Result<(), StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store.set_stats_target(site, entity, columns, target)
    }

    pub async fn create_manual_index(
        &self,
        deployment: &DeploymentLocator,
        entity_name: &str,
        field_names: Vec<String>,
        index_method: Method,
        after: Option<BlockNumber>,
    ) -> Result<(), StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store
            .create_manual_index(site, entity_name, field_names, index_method, after)
            .await
    }

    pub async fn indexes_for_entity(
        &self,
        deployment: &DeploymentLocator,
        entity_name: &str,
    ) -> Result<Vec<CreateIndex>, StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store.indexes_for_entity(site, entity_name).await
    }

    pub async fn drop_index_for_deployment(
        &self,
        deployment: &DeploymentLocator,
        index_name: &str,
    ) -> Result<(), StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store.drop_index(site, index_name).await
    }

    pub async fn set_account_like(
        &self,
        deployment: &DeploymentLocator,
        table: &str,
        is_account_like: bool,
    ) -> Result<(), StoreError> {
        let (store, site) = self.store(&deployment.hash)?;
        store.set_account_like(site, table, is_account_like).await
    }

    /// Prune the history according to the parameters in `req`.
    ///
    /// Pruning can take a long time, and is structured into multiple
    /// transactions such that none of them takes an excessively long time.
    /// If pruning gets interrupted, it may leave some intermediate tables
    /// in the database behind. Those will get cleaned up the next time an
    /// attempt is made to prune the same deployment.
    pub async fn prune(
        &self,
        reporter: Box<dyn PruneReporter>,
        deployment: &DeploymentLocator,
        req: PruneRequest,
    ) -> Result<Box<dyn PruneReporter>, StoreError> {
        // Find the store by the deployment id; otherwise, we could only
        // prune the active copy of the deployment with `deployment.hash`
        let site = self.find_site(deployment.id.into())?;
        let store = self.for_site(&site)?;

        store.prune(reporter, site, req).await
    }

    pub fn set_history_blocks(
        &self,
        deployment: &DeploymentLocator,
        history_blocks: BlockNumber,
        reorg_threshold: BlockNumber,
    ) -> Result<(), StoreError> {
        let site = self.find_site(deployment.id.into())?;
        let store = self.for_site(&site)?;

        store.set_history_blocks(&site, history_blocks, reorg_threshold)
    }

    pub fn load_deployment(&self, site: Arc<Site>) -> Result<SubgraphDeploymentEntity, StoreError> {
        let src_store = self.for_site(&site)?;
        src_store.load_deployment(site)
    }

    pub fn load_deployment_by_id(
        &self,
        id: DeploymentId,
    ) -> Result<SubgraphDeploymentEntity, StoreError> {
        let site = self.find_site(id)?;
        let src_store = self.for_site(&site)?;
        src_store.load_deployment(site)
    }
}

const STATE_ENS_NOT_CHECKED: u8 = 0;
const STATE_ENS_EMPTY: u8 = 1;
const STATE_ENS_NOT_EMPTY: u8 = 2;

/// EnsLookup reads from a rainbow table store in postgres that needs to be manually
/// loaded. To avoid unnecessary database roundtrips, the empty table check is lazy
/// and will not be retried. Once the table is checked, any subsequent calls will
/// just used the stored result.
struct EnsLookup {
    primary: ConnectionPool,
    // In order to keep the struct lock free, we'll use u8 for the status:
    // 0 - Not Checked
    // 1 - Checked - empty
    // 2 - Checked - non empty
    state: AtomicU8,
}

impl EnsLookup {
    pub fn new(pool: ConnectionPool) -> Self {
        Self {
            primary: pool,
            state: AtomicU8::new(STATE_ENS_NOT_CHECKED),
        }
    }

    fn is_table_empty(pool: &ConnectionPool) -> Result<bool, StoreError> {
        let conn = pool.get()?;
        primary::Connection::new(conn).is_ens_table_empty()
    }
}

impl EnsLookupTrait for EnsLookup {
    fn find_name(&self, hash: &str) -> Result<Option<String>, StoreError> {
        let conn = self.primary.get()?;
        primary::Connection::new(conn).find_ens_name(hash)
    }

    fn is_table_empty(&self) -> Result<bool, StoreError> {
        match self.state.load(std::sync::atomic::Ordering::SeqCst) {
            STATE_ENS_NOT_CHECKED => {}
            STATE_ENS_EMPTY => return Ok(true),
            STATE_ENS_NOT_EMPTY => return Ok(false),
            _ => unreachable!("unsupported state"),
        }

        let is_empty = Self::is_table_empty(&self.primary)?;
        let new_state = match is_empty {
            true => STATE_ENS_EMPTY,
            false => STATE_ENS_NOT_EMPTY,
        };
        self.state
            .store(new_state, std::sync::atomic::Ordering::SeqCst);

        Ok(is_empty)
    }
}

#[async_trait::async_trait]
impl SubgraphStoreTrait for SubgraphStore {
    fn ens_lookup(&self) -> Arc<dyn EnsLookupTrait> {
        Arc::new(EnsLookup::new(self.mirror.primary().clone()))
    }

    // FIXME: This method should not get a node_id
    fn create_subgraph_deployment(
        &self,
        name: SubgraphName,
        schema: &InputSchema,
        deployment: DeploymentCreate,
        node_id: NodeId,
        network_name: String,
        mode: SubgraphVersionSwitchingMode,
    ) -> Result<DeploymentLocator, StoreError> {
        self.create_deployment_internal(
            name,
            schema,
            deployment,
            node_id,
            network_name,
            mode,
            false,
        )
    }

    fn create_subgraph(&self, name: SubgraphName) -> Result<String, StoreError> {
        let pconn = self.primary_conn()?;
        pconn.transaction(|| pconn.create_subgraph(&name))
    }

    fn create_subgraph_features(&self, features: DeploymentFeatures) -> Result<(), StoreError> {
        let pconn = self.primary_conn()?;
        pconn.transaction(|| pconn.create_subgraph_features(features))
    }

    fn remove_subgraph(&self, name: SubgraphName) -> Result<(), StoreError> {
        let pconn = self.primary_conn()?;
        pconn.transaction(|| -> Result<_, StoreError> {
            let changes = pconn.remove_subgraph(name)?;
            pconn.send_store_event(&self.sender, &StoreEvent::new(changes))
        })
    }

    fn reassign_subgraph(
        &self,
        deployment: &DeploymentLocator,
        node_id: &NodeId,
    ) -> Result<(), StoreError> {
        let site = self.find_site(deployment.id.into())?;
        let pconn = self.primary_conn()?;
        pconn.transaction(|| -> Result<_, StoreError> {
            let changes = pconn.reassign_subgraph(site.as_ref(), node_id)?;
            pconn.send_store_event(&self.sender, &StoreEvent::new(changes))
        })
    }

    fn assigned_node(&self, deployment: &DeploymentLocator) -> Result<Option<NodeId>, StoreError> {
        let site = self.find_site(deployment.id.into())?;
        self.mirror.assigned_node(site.as_ref())
    }

    /// Returns Option<(node_id,is_paused)> where `node_id` is the node that
    /// the subgraph is assigned to, and `is_paused` is true if the
    /// subgraph is paused.
    /// Returns None if the deployment does not exist.
    fn assignment_status(
        &self,
        deployment: &DeploymentLocator,
    ) -> Result<Option<(NodeId, bool)>, StoreError> {
        let site = self.find_site(deployment.id.into())?;
        self.mirror.assignment_status(site.as_ref())
    }

    fn assignments(&self, node: &NodeId) -> Result<Vec<DeploymentLocator>, StoreError> {
        self.mirror
            .assignments(node)
            .map(|sites| sites.iter().map(|site| site.into()).collect())
    }

    fn active_assignments(&self, node: &NodeId) -> Result<Vec<DeploymentLocator>, StoreError> {
        self.mirror
            .active_assignments(node)
            .map(|sites| sites.iter().map(|site| site.into()).collect())
    }

    fn subgraph_exists(&self, name: &SubgraphName) -> Result<bool, StoreError> {
        self.mirror.subgraph_exists(name)
    }

    async fn subgraph_features(
        &self,
        deployment: &DeploymentHash,
    ) -> Result<Option<DeploymentFeatures>, StoreError> {
        let deployment = deployment.to_string();
        self.with_primary_conn(|conn| {
            conn.transaction(|| conn.get_subgraph_features(deployment).map_err(|e| e.into()))
        })
        .await
    }

    fn entity_changes_in_block(
        &self,
        subgraph_id: &DeploymentHash,
        block: BlockNumber,
    ) -> Result<Vec<EntityOperation>, StoreError> {
        let (store, site) = self.store(subgraph_id)?;
        let changes = store.get_changes(site, block)?;
        Ok(changes)
    }

    fn input_schema(&self, id: &DeploymentHash) -> Result<InputSchema, StoreError> {
        let (store, site) = self.store(id)?;
        let info = store.subgraph_info(&site)?;
        Ok(info.input)
    }

    fn api_schema(
        &self,
        id: &DeploymentHash,
        version: &ApiVersion,
    ) -> Result<Arc<ApiSchema>, StoreError> {
        let (store, site) = self.store(id)?;
        let info = store.subgraph_info(&site)?;
        Ok(info.api.get(version).unwrap().clone())
    }

    fn debug_fork(
        &self,
        id: &DeploymentHash,
        logger: Logger,
    ) -> Result<Option<Arc<dyn SubgraphFork>>, StoreError> {
        let (store, site) = self.store(id)?;
        let info = store.subgraph_info(&site)?;
        let fork_id = info.debug_fork;
        let schema = info.input;

        match (self.fork_base.as_ref(), fork_id) {
            (Some(base), Some(id)) => Ok(Some(Arc::new(fork::SubgraphFork::new(
                base.clone(),
                id,
                schema,
                logger,
            )?))),
            _ => Ok(None),
        }
    }

    async fn writable(
        self: Arc<Self>,
        logger: Logger,
        deployment: graph::components::store::DeploymentId,
        manifest_idx_and_name: Arc<Vec<(u32, String)>>,
    ) -> Result<Arc<dyn store::WritableStore>, StoreError> {
        let deployment = deployment.into();
        // We cache writables to make sure calls to this method are
        // idempotent and there is ever only one `WritableStore` for any
        // deployment
        if let Some(writable) = self.writables.lock().unwrap().get(&deployment) {
            // A poisoned writable will not write anything anymore; we
            // discard it and create a new one that is properly initialized
            // according to the state in the database.
            if !writable.poisoned() {
                return Ok(writable.cheap_clone());
            }
        }

        // Ideally the lower level functions would be asyncified.
        let this = self.clone();
        let site = graph::spawn_blocking_allow_panic(move || -> Result<_, StoreError> {
            this.find_site(deployment)
        })
        .await
        .unwrap()?; // Propagate panics, there shouldn't be any.

        let writable = Arc::new(
            WritableStore::new(
                self.as_ref().clone(),
                logger,
                site,
                manifest_idx_and_name,
                self.registry.clone(),
            )
            .await?,
        );
        self.writables
            .lock()
            .unwrap()
            .insert(deployment, writable.cheap_clone());
        Ok(writable)
    }

    async fn stop_subgraph(&self, loc: &DeploymentLocator) -> Result<(), StoreError> {
        self.evict(&loc.hash)?;

        // Remove the writable from the cache and stop it
        let deployment = loc.id.into();
        let writable = self.writables.lock().unwrap().remove(&deployment);
        match writable {
            Some(writable) => writable.stop().await,
            None => Ok(()),
        }
    }

    fn is_deployed(&self, id: &DeploymentHash) -> Result<bool, StoreError> {
        match self.site(id) {
            Ok(_) => Ok(true),
            Err(StoreError::DeploymentNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn graft_pending(&self, id: &DeploymentHash) -> Result<bool, StoreError> {
        let (store, _) = self.store(id)?;
        let graft_detail = store.graft_pending(id)?;
        Ok(graft_detail.is_some())
    }

    async fn least_block_ptr(&self, id: &DeploymentHash) -> Result<Option<BlockPtr>, StoreError> {
        let (store, site) = self.store(id)?;
        store.block_ptr(site.cheap_clone()).await
    }

    async fn is_healthy(&self, id: &DeploymentHash) -> Result<bool, StoreError> {
        let (store, site) = self.store(id)?;
        let health = store.health(&site).await?;
        Ok(matches!(health, SubgraphHealth::Healthy))
    }

    /// Find the deployment locators for the subgraph with the given hash
    fn locators(&self, hash: &str) -> Result<Vec<DeploymentLocator>, StoreError> {
        Ok(self
            .mirror
            .find_sites(&[hash.to_string()], false)?
            .iter()
            .map(|site| site.into())
            .collect())
    }

    fn active_locator(&self, hash: &str) -> Result<Option<DeploymentLocator>, StoreError> {
        let sites = self.mirror.find_sites(&[hash.to_string()], true)?;
        if sites.len() > 1 {
            return Err(constraint_violation!(
                "There are {} active deployments for {hash}, there should only be one",
                sites.len()
            ));
        }
        Ok(sites.first().map(DeploymentLocator::from))
    }

    async fn set_manifest_raw_yaml(
        &self,
        hash: &DeploymentHash,
        raw_yaml: String,
    ) -> Result<(), StoreError> {
        let (store, site) = self.store(hash)?;
        store.set_manifest_raw_yaml(site, raw_yaml).await
    }

    fn instrument(&self, deployment: &DeploymentLocator) -> Result<bool, StoreError> {
        let site = self.find_site(deployment.id.into())?;
        let store = self.for_site(&site)?;

        let info = store.subgraph_info(&site)?;
        Ok(info.instrument)
    }
}
