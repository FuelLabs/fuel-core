//! Queries to support the index node API
//!
// For git_testament_macros
#![allow(unused_macros)]
use diesel::dsl;
use diesel::prelude::{
    ExpressionMethods, JoinOnDsl, NullableExpressionMethods, OptionalExtension, PgConnection,
    QueryDsl, RunQueryDsl,
};
use diesel_derives::Associations;
use git_testament::{git_testament, git_testament_macros};
use graph::blockchain::BlockHash;
use graph::data::subgraph::schema::{SubgraphError, SubgraphManifestEntity};
use graph::prelude::{
    bigdecimal::ToPrimitive, BigDecimal, BlockPtr, DeploymentHash, StoreError,
    SubgraphDeploymentEntity,
};
use graph::schema::InputSchema;
use graph::{constraint_violation, data::subgraph::status, prelude::web3::types::H256};
use itertools::Itertools;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::{ops::Bound, sync::Arc};

use crate::deployment::{
    graph_node_versions, subgraph_deployment, subgraph_error, subgraph_manifest,
    SubgraphHealth as HealthType,
};
use crate::primary::{DeploymentId, Site};

git_testament_macros!(version);
git_testament!(TESTAMENT);

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const CARGO_PKG_VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const CARGO_PKG_VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const CARGO_PKG_VERSION_PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");

type Bytes = Vec<u8>;

#[derive(Queryable, QueryableByName)]
#[table_name = "subgraph_deployment"]
// We map all fields to make loading `Detail` with diesel easier, but we
// don't need all the fields
#[allow(dead_code)]
pub struct DeploymentDetail {
    pub id: DeploymentId,
    pub deployment: String,
    pub failed: bool,
    health: HealthType,
    pub synced: bool,
    fatal_error: Option<String>,
    non_fatal_errors: Vec<String>,
    /// The earliest block for which we have history
    earliest_block_number: i32,
    pub latest_ethereum_block_hash: Option<Bytes>,
    pub latest_ethereum_block_number: Option<BigDecimal>,
    last_healthy_ethereum_block_hash: Option<Bytes>,
    last_healthy_ethereum_block_number: Option<BigDecimal>,
    pub entity_count: BigDecimal,
    graft_base: Option<String>,
    graft_block_hash: Option<Bytes>,
    graft_block_number: Option<BigDecimal>,
    debug_fork: Option<String>,
    reorg_count: i32,
    current_reorg_depth: i32,
    max_reorg_depth: i32,
    firehose_cursor: Option<String>,
}

#[derive(Queryable, QueryableByName)]
#[table_name = "subgraph_error"]
// We map all fields to make loading `Detail` with diesel easier, but we
// don't need all the fields
#[allow(dead_code)]
pub(crate) struct ErrorDetail {
    vid: i64,
    pub id: String,
    subgraph_id: String,
    message: String,
    pub block_hash: Option<Bytes>,
    handler: Option<String>,
    pub deterministic: bool,
    pub block_range: (Bound<i32>, Bound<i32>),
}

impl ErrorDetail {
    /// Fetches the fatal error, if present, associated with the given
    /// [`DeploymentHash`].
    pub fn fatal(
        conn: &PgConnection,
        deployment_id: &DeploymentHash,
    ) -> Result<Option<Self>, StoreError> {
        use subgraph_deployment as d;
        use subgraph_error as e;

        d::table
            .filter(d::deployment.eq(deployment_id.as_str()))
            .inner_join(e::table.on(e::id.nullable().eq(d::fatal_error)))
            .select(e::all_columns)
            .get_result(conn)
            .optional()
            .map_err(StoreError::from)
    }
}

impl TryFrom<ErrorDetail> for SubgraphError {
    type Error = StoreError;

    fn try_from(value: ErrorDetail) -> Result<Self, Self::Error> {
        let ErrorDetail {
            vid: _,
            id: _,
            subgraph_id,
            message,
            block_hash,
            handler,
            deterministic,
            block_range,
        } = value;
        let block_number = crate::block_range::first_block_in_range(&block_range);
        // FIXME:
        //
        // workaround for arweave
        let block_hash = block_hash.map(|hash| H256::from_slice(&hash.as_slice()[..32]));
        // In existing databases, we have errors that have a `block_range` of
        // `UNVERSIONED_RANGE`, which leads to `None` as the block number, but
        // has a hash. Conversely, it is also possible for an error to not have a
        // hash. In both cases, use a block pointer of `None`
        let block_ptr = match (block_number, block_hash) {
            (Some(number), Some(hash)) => Some(BlockPtr::from((hash, number as u64))),
            _ => None,
        };
        let subgraph_id = DeploymentHash::new(subgraph_id).map_err(|id| {
            StoreError::ConstraintViolation(format!("invalid subgraph id `{}` in fatal error", id))
        })?;
        Ok(SubgraphError {
            subgraph_id,
            message,
            block_ptr,
            handler,
            deterministic,
        })
    }
}

pub(crate) fn block(
    id: &str,
    name: &str,
    hash: Option<Vec<u8>>,
    number: Option<BigDecimal>,
) -> Result<Option<status::EthereumBlock>, StoreError> {
    match (hash, number) {
        (Some(hash), Some(number)) => {
            let number = number.to_i32().ok_or_else(|| {
                constraint_violation!(
                    "the block number {} for {} in {} is not representable as an i32",
                    number,
                    name,
                    id
                )
            })?;
            Ok(Some(status::EthereumBlock::new(
                BlockHash(hash.into_boxed_slice()),
                number,
            )))
        }
        (None, None) => Ok(None),
        (hash, number) => Err(constraint_violation!(
            "the hash and number \
        of a block pointer must either both be null or both have a \
        value, but for `{}` the hash of {} is `{:?}` and the number is `{:?}`",
            id,
            name,
            hash,
            number
        )),
    }
}

pub(crate) fn info_from_details(
    detail: DeploymentDetail,
    fatal: Option<ErrorDetail>,
    non_fatal: Vec<ErrorDetail>,
    sites: &[Arc<Site>],
    subgraph_history_blocks: i32,
) -> Result<status::Info, StoreError> {
    let DeploymentDetail {
        id,
        deployment,
        failed: _,
        health,
        synced,
        fatal_error: _,
        non_fatal_errors: _,
        earliest_block_number,
        latest_ethereum_block_hash,
        latest_ethereum_block_number,
        entity_count,
        graft_base: _,
        graft_block_hash: _,
        graft_block_number: _,
        ..
    } = detail;

    let site = sites
        .iter()
        .find(|site| site.deployment.as_str() == deployment)
        .ok_or_else(|| constraint_violation!("missing site for subgraph `{}`", deployment))?;

    // This needs to be filled in later since it lives in a
    // different shard
    let chain_head_block = None;
    let latest_block = block(
        &deployment,
        "latest_ethereum_block",
        latest_ethereum_block_hash,
        latest_ethereum_block_number,
    )?;
    let health = health.into();
    let chain = status::ChainInfo {
        network: site.network.clone(),
        chain_head_block,
        earliest_block_number,
        latest_block,
    };
    let entity_count = entity_count.to_u64().ok_or_else(|| {
        constraint_violation!(
            "the entityCount for {} is not representable as a u64",
            deployment
        )
    })?;
    let fatal_error = fatal.map(SubgraphError::try_from).transpose()?;
    let non_fatal_errors = non_fatal
        .into_iter()
        .map(SubgraphError::try_from)
        .collect::<Result<Vec<SubgraphError>, StoreError>>()?;

    // 'node' needs to be filled in later from a different shard
    Ok(status::Info {
        id: id.into(),
        subgraph: deployment,
        synced,
        health,
        paused: None,
        fatal_error,
        non_fatal_errors,
        chains: vec![chain],
        entity_count,
        node: None,
        history_blocks: subgraph_history_blocks,
    })
}

/// Return the details for `deployments`
pub(crate) fn deployment_details(
    conn: &PgConnection,
    deployments: Vec<String>,
) -> Result<Vec<DeploymentDetail>, StoreError> {
    use subgraph_deployment as d;

    // Empty deployments means 'all of them'
    let details = if deployments.is_empty() {
        d::table.load::<DeploymentDetail>(conn)?
    } else {
        d::table
            .filter(d::deployment.eq_any(&deployments))
            .load::<DeploymentDetail>(conn)?
    };
    Ok(details)
}

pub(crate) fn deployment_statuses(
    conn: &PgConnection,
    sites: &[Arc<Site>],
) -> Result<Vec<status::Info>, StoreError> {
    use subgraph_deployment as d;
    use subgraph_error as e;
    use subgraph_manifest as sm;

    // First, we fetch all deployment information along with any fatal errors.
    // Subsequently, we fetch non-fatal errors and we group them by deployment
    // ID.

    let details_with_fatal_error = {
        let join = e::table.on(e::id.nullable().eq(d::fatal_error));

        // Empty deployments means 'all of them'
        if sites.is_empty() {
            d::table
                .left_outer_join(join)
                .load::<(DeploymentDetail, Option<ErrorDetail>)>(conn)?
        } else {
            d::table
                .left_outer_join(join)
                .filter(d::id.eq_any(sites.iter().map(|site| site.id)))
                .load::<(DeploymentDetail, Option<ErrorDetail>)>(conn)?
        }
    };

    let mut non_fatal_errors = {
        let join = e::table.on(e::id.eq(dsl::any(d::non_fatal_errors)));

        if sites.is_empty() {
            d::table
                .inner_join(join)
                .select((d::id, e::all_columns))
                .load::<(DeploymentId, ErrorDetail)>(conn)?
        } else {
            d::table
                .inner_join(join)
                .filter(d::id.eq_any(sites.iter().map(|site| site.id)))
                .select((d::id, e::all_columns))
                .load::<(DeploymentId, ErrorDetail)>(conn)?
        }
        .into_iter()
        .into_group_map()
    };

    let mut history_blocks_map: HashMap<_, _> = {
        if sites.is_empty() {
            sm::table
                .select((sm::id, sm::history_blocks))
                .load::<(DeploymentId, i32)>(conn)?
        } else {
            sm::table
                .filter(sm::id.eq_any(sites.iter().map(|site| site.id)))
                .select((sm::id, sm::history_blocks))
                .load::<(DeploymentId, i32)>(conn)?
        }
        .into_iter()
        .collect()
    };

    details_with_fatal_error
        .into_iter()
        .map(|(detail, fatal)| {
            let non_fatal = non_fatal_errors.remove(&detail.id).unwrap_or_default();
            let subgraph_history_blocks = history_blocks_map.remove(&detail.id).unwrap_or_default();
            info_from_details(detail, fatal, non_fatal, sites, subgraph_history_blocks)
        })
        .collect()
}

#[derive(Queryable, QueryableByName, Identifiable, Associations)]
#[table_name = "subgraph_manifest"]
#[belongs_to(GraphNodeVersion)]
// We never read the id field but map it to make the interaction with Diesel
// simpler
#[allow(dead_code)]
struct StoredSubgraphManifest {
    id: i32,
    spec_version: String,
    description: Option<String>,
    repository: Option<String>,
    features: Vec<String>,
    schema: String,
    graph_node_version_id: Option<i32>,
    use_bytea_prefix: bool,
    start_block_number: Option<i32>,
    start_block_hash: Option<Bytes>,
    raw_yaml: Option<String>,
    entities_with_causality_region: Vec<String>,
    on_sync: Option<String>,
    history_blocks: i32,
}

impl StoredSubgraphManifest {
    fn as_manifest(self, schema: &InputSchema) -> SubgraphManifestEntity {
        let e: Vec<_> = self
            .entities_with_causality_region
            .into_iter()
            .map(|s| schema.entity_type(&s).unwrap())
            .collect();
        SubgraphManifestEntity {
            spec_version: self.spec_version,
            description: self.description,
            repository: self.repository,
            features: self.features,
            schema: self.schema,
            raw_yaml: self.raw_yaml,
            entities_with_causality_region: e,
            history_blocks: self.history_blocks,
        }
    }
}

struct StoredDeploymentEntity(crate::detail::DeploymentDetail, StoredSubgraphManifest);

impl StoredDeploymentEntity {
    fn as_subgraph_deployment(
        self,
        schema: &InputSchema,
    ) -> Result<SubgraphDeploymentEntity, StoreError> {
        let (detail, manifest) = (self.0, self.1);

        let start_block = block(
            &detail.deployment,
            "start_block",
            manifest.start_block_hash.clone(),
            manifest.start_block_number.map(|n| n.into()),
        )?
        .map(|block| block.to_ptr());

        let latest_block = block(
            &detail.deployment,
            "latest_block",
            detail.latest_ethereum_block_hash,
            detail.latest_ethereum_block_number,
        )?
        .map(|block| block.to_ptr());

        let graft_block = block(
            &detail.deployment,
            "graft_block",
            detail.graft_block_hash,
            detail.graft_block_number,
        )?
        .map(|block| block.to_ptr());

        let graft_base = detail
            .graft_base
            .map(DeploymentHash::new)
            .transpose()
            .map_err(|b| constraint_violation!("invalid graft base `{}`", b))?;

        let debug_fork = detail
            .debug_fork
            .map(DeploymentHash::new)
            .transpose()
            .map_err(|b| constraint_violation!("invalid debug fork `{}`", b))?;

        Ok(SubgraphDeploymentEntity {
            manifest: manifest.as_manifest(schema),
            failed: detail.failed,
            health: detail.health.into(),
            synced: detail.synced,
            fatal_error: None,
            non_fatal_errors: vec![],
            earliest_block_number: detail.earliest_block_number,
            start_block,
            latest_block,
            graft_base,
            graft_block,
            debug_fork,
            reorg_count: detail.reorg_count,
            current_reorg_depth: detail.current_reorg_depth,
            max_reorg_depth: detail.max_reorg_depth,
        })
    }
}

pub fn deployment_entity(
    conn: &PgConnection,
    site: &Site,
    schema: &InputSchema,
) -> Result<SubgraphDeploymentEntity, StoreError> {
    use subgraph_deployment as d;
    use subgraph_manifest as m;

    let manifest = m::table
        .find(site.id)
        .first::<StoredSubgraphManifest>(conn)?;

    let detail = d::table
        .find(site.id)
        .first::<crate::detail::DeploymentDetail>(conn)?;

    StoredDeploymentEntity(detail, manifest).as_subgraph_deployment(schema)
}

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "graph_node_versions"]
pub struct GraphNodeVersion {
    pub id: i32,
    pub git_commit_hash: String,
    pub git_repository_dirty: bool,
    pub crate_version: String,
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
}

impl GraphNodeVersion {
    pub(crate) fn create_or_get(conn: &PgConnection) -> anyhow::Result<i32> {
        let git_commit_hash = version_commit_hash!();
        let git_repository_dirty = !&TESTAMENT.modifications.is_empty();
        let crate_version = CARGO_PKG_VERSION;
        let major: i32 = CARGO_PKG_VERSION_MAJOR
            .parse()
            .expect("failed to parse cargo major package version");
        let minor: i32 = CARGO_PKG_VERSION_MINOR
            .parse()
            .expect("failed to parse cargo major package version");
        let patch: i32 = CARGO_PKG_VERSION_PATCH
            .parse()
            .expect("failed to parse cargo major package version");

        let graph_node_version_id = {
            use graph_node_versions::dsl as g;

            // try to insert our current values
            diesel::insert_into(g::graph_node_versions)
                .values((
                    g::git_commit_hash.eq(&git_commit_hash),
                    g::git_repository_dirty.eq(git_repository_dirty),
                    g::crate_version.eq(&crate_version),
                    g::major.eq(&major),
                    g::minor.eq(&minor),
                    g::patch.eq(&patch),
                ))
                .on_conflict_do_nothing()
                .execute(conn)?;

            // select the id for the row we just inserted
            g::graph_node_versions
                .select(g::id)
                .filter(g::git_commit_hash.eq(&git_commit_hash))
                .filter(g::git_repository_dirty.eq(git_repository_dirty))
                .filter(g::crate_version.eq(&crate_version))
                .filter(g::major.eq(&major))
                .filter(g::minor.eq(&minor))
                .filter(g::patch.eq(&patch))
                .get_result(conn)?
        };
        Ok(graph_node_version_id)
    }
}
