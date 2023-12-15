//! Utilities for dealing with deployment metadata. Any connection passed
//! into these methods must be for the shard that holds the actual
//! deployment data and metadata
use crate::{advisory_lock, detail::GraphNodeVersion, primary::DeploymentId};
use diesel::{
    connection::SimpleConnection,
    dsl::{count, delete, insert_into, select, sql, update},
    sql_types::Integer,
};
use diesel::{expression::SqlLiteral, pg::PgConnection, sql_types::Numeric};
use diesel::{
    prelude::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl},
    sql_query,
    sql_types::{Nullable, Text},
};
use graph::{
    blockchain::block_stream::FirehoseCursor, data::subgraph::schema::SubgraphError,
    schema::EntityType,
};
use graph::{
    data::subgraph::{
        schema::{DeploymentCreate, SubgraphManifestEntity},
        SubgraphFeature,
    },
    util::backoff::ExponentialBackoff,
};
use graph::{
    prelude::{
        anyhow, bigdecimal::ToPrimitive, hex, web3::types::H256, BigDecimal, BlockNumber, BlockPtr,
        DeploymentHash, DeploymentState, StoreError,
    },
    schema::InputSchema,
};
use stable_hash_legacy::crypto::SetHasher;
use std::{collections::BTreeSet, convert::TryFrom, ops::Bound, time::Duration};
use std::{str::FromStr, sync::Arc};

use crate::connection_pool::ForeignServer;
use crate::{block_range::BLOCK_RANGE_COLUMN, primary::Site};
use graph::constraint_violation;

#[derive(DbEnum, Debug, Clone, Copy)]
pub enum SubgraphHealth {
    Failed,
    Healthy,
    Unhealthy,
}

impl SubgraphHealth {
    fn is_failed(&self) -> bool {
        use graph::data::subgraph::schema::SubgraphHealth as H;

        H::from(*self).is_failed()
    }
}

impl From<SubgraphHealth> for graph::data::subgraph::schema::SubgraphHealth {
    fn from(health: SubgraphHealth) -> Self {
        use graph::data::subgraph::schema::SubgraphHealth as H;
        use SubgraphHealth as Db;

        match health {
            Db::Failed => H::Failed,
            Db::Healthy => H::Healthy,
            Db::Unhealthy => H::Unhealthy,
        }
    }
}

/// Additional behavior for a deployment when it becomes synced
#[derive(Clone, Copy, Debug)]
pub enum OnSync {
    None,
    /// Activate this deployment
    Activate,
    /// Activate this deployment and unassign any other copies of the same
    /// deployment
    Replace,
}

impl TryFrom<Option<&str>> for OnSync {
    type Error = StoreError;

    fn try_from(value: Option<&str>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(OnSync::None),
            Some("activate") => Ok(OnSync::Activate),
            Some("replace") => Ok(OnSync::Replace),
            _ => Err(constraint_violation!("illegal value for on_sync: {value}")),
        }
    }
}

impl OnSync {
    pub fn activate(&self) -> bool {
        match self {
            OnSync::None => false,
            OnSync::Activate => true,
            OnSync::Replace => true,
        }
    }

    pub fn replace(&self) -> bool {
        match self {
            OnSync::None => false,
            OnSync::Activate => false,
            OnSync::Replace => true,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            OnSync::None => "none",
            OnSync::Activate => "activate",
            OnSync::Replace => "replace",
        }
    }

    fn to_sql(&self) -> Option<&str> {
        match self {
            OnSync::None => None,
            OnSync::Activate | OnSync::Replace => Some(self.to_str()),
        }
    }
}

table! {
    subgraphs.subgraph_deployment (id) {
        id -> Integer,
        deployment -> Text,
        failed -> Bool,
        health -> crate::deployment::SubgraphHealthMapping,
        synced -> Bool,
        fatal_error -> Nullable<Text>,
        non_fatal_errors -> Array<Text>,
        earliest_block_number -> Integer,
        latest_ethereum_block_hash -> Nullable<Binary>,
        latest_ethereum_block_number -> Nullable<Numeric>,
        last_healthy_ethereum_block_hash -> Nullable<Binary>,
        last_healthy_ethereum_block_number -> Nullable<Numeric>,
        entity_count -> Numeric,
        graft_base -> Nullable<Text>,
        graft_block_hash -> Nullable<Binary>,
        graft_block_number -> Nullable<Numeric>,
        debug_fork -> Nullable<Text>,
        reorg_count -> Integer,
        current_reorg_depth -> Integer,
        max_reorg_depth -> Integer,
        firehose_cursor -> Nullable<Text>,
    }
}

table! {
    subgraphs.subgraph_error (vid) {
        vid -> BigInt,
        id -> Text,
        subgraph_id -> Text,
        message -> Text,
        block_hash -> Nullable<Binary>,
        handler -> Nullable<Text>,
        deterministic -> Bool,
        block_range -> Range<Integer>,
    }
}

table! {
    subgraphs.subgraph_manifest {
        id -> Integer,
        spec_version -> Text,
        description -> Nullable<Text>,
        repository -> Nullable<Text>,
        features -> Array<Text>,
        schema -> Text,
        graph_node_version_id -> Nullable<Integer>,
        use_bytea_prefix -> Bool,
        /// Parent of the smallest start block from the manifest
        start_block_number -> Nullable<Integer>,
        start_block_hash -> Nullable<Binary>,
        raw_yaml -> Nullable<Text>,

        // Entity types that have a `causality_region` column.
        // Names stored as present in the schema, not in snake case.
        entities_with_causality_region -> Array<Text>,
        on_sync -> Nullable<Text>,
        // How many blocks of history to keep, defaults to `i32::max` for
        // unlimited history
        history_blocks -> Integer,
    }
}

table! {
    subgraphs.graph_node_versions {
        id -> Integer,
        git_commit_hash -> Text,
        git_repository_dirty -> Bool,
        crate_version -> Text,
        major -> Integer,
        minor -> Integer,
        patch -> Integer,
    }
}

allow_tables_to_appear_in_same_query!(subgraph_deployment, subgraph_error, subgraph_manifest);

/// Look up the graft point for the given subgraph in the database and
/// return it. If `pending_only` is `true`, only return `Some(_)` if the
/// deployment has not progressed past the graft point, i.e., data has not
/// been copied for the graft
fn graft(
    conn: &PgConnection,
    id: &DeploymentHash,
    pending_only: bool,
) -> Result<Option<(DeploymentHash, BlockPtr)>, StoreError> {
    use subgraph_deployment as sd;

    let graft_query = sd::table
        .select((sd::graft_base, sd::graft_block_hash, sd::graft_block_number))
        .filter(sd::deployment.eq(id.as_str()));
    // The name of the base subgraph, the hash, and block number
    let graft: (Option<String>, Option<Vec<u8>>, Option<BigDecimal>) = if pending_only {
        graft_query
            .filter(sd::latest_ethereum_block_number.is_null())
            .first(conn)
            .optional()?
            .unwrap_or((None, None, None))
    } else {
        graft_query
            .first(conn)
            .optional()?
            .unwrap_or((None, None, None))
    };
    match graft {
        (None, None, None) => Ok(None),
        (Some(subgraph), Some(hash), Some(block)) => {
            // FIXME:
            //
            // workaround for arweave
            let hash = H256::from_slice(&hash.as_slice()[..32]);
            let block = block.to_u64().expect("block numbers fit into a u64");
            let subgraph = DeploymentHash::new(subgraph.clone()).map_err(|_| {
                StoreError::Unknown(anyhow!(
                    "the base subgraph for a graft must be a valid subgraph id but is `{}`",
                    subgraph
                ))
            })?;
            Ok(Some((subgraph, BlockPtr::from((hash, block)))))
        }
        _ => unreachable!(
            "graftBlockHash and graftBlockNumber are either both set or neither is set"
        ),
    }
}

/// Look up the graft point for the given subgraph in the database and
/// return it. Returns `None` if the deployment does not have
/// a graft or if the subgraph has already progress past the graft point,
/// indicating that the data copying for grafting has been performed
pub fn graft_pending(
    conn: &PgConnection,
    id: &DeploymentHash,
) -> Result<Option<(DeploymentHash, BlockPtr)>, StoreError> {
    graft(conn, id, true)
}

/// Look up the graft point for the given subgraph in the database and
/// return it. Returns `None` if the deployment does not have
/// a graft.
pub fn graft_point(
    conn: &PgConnection,
    id: &DeploymentHash,
) -> Result<Option<(DeploymentHash, BlockPtr)>, StoreError> {
    graft(conn, id, false)
}

/// Look up the debug fork for the given subgraph in the database and
/// return it. Returns `None` if the deployment does not have
/// a debug fork.
pub fn debug_fork(
    conn: &PgConnection,
    id: &DeploymentHash,
) -> Result<Option<DeploymentHash>, StoreError> {
    use subgraph_deployment as sd;

    let debug_fork: Option<String> = sd::table
        .select(sd::debug_fork)
        .filter(sd::deployment.eq(id.as_str()))
        .first(conn)?;

    match debug_fork {
        Some(fork) => Ok(Some(DeploymentHash::new(fork.clone()).map_err(|_| {
            StoreError::Unknown(anyhow!(
                "the debug fork for a subgraph must be a valid subgraph id but is `{}`",
                fork
            ))
        })?)),
        None => Ok(None),
    }
}

pub fn schema(conn: &PgConnection, site: &Site) -> Result<(InputSchema, bool), StoreError> {
    use subgraph_manifest as sm;
    let (s, use_bytea_prefix) = sm::table
        .select((sm::schema, sm::use_bytea_prefix))
        .filter(sm::id.eq(site.id))
        .first::<(String, bool)>(conn)?;
    InputSchema::parse(s.as_str(), site.deployment.clone())
        .map_err(StoreError::Unknown)
        .map(|schema| (schema, use_bytea_prefix))
}

pub struct ManifestInfo {
    pub input_schema: InputSchema,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub spec_version: String,
    pub instrument: bool,
}

impl ManifestInfo {
    pub fn load(conn: &PgConnection, site: &Site) -> Result<ManifestInfo, StoreError> {
        use subgraph_manifest as sm;
        let (s, description, repository, spec_version, features): (
            String,
            Option<String>,
            Option<String>,
            String,
            Vec<String>,
        ) = sm::table
            .select((
                sm::schema,
                sm::description,
                sm::repository,
                sm::spec_version,
                sm::features,
            ))
            .filter(sm::id.eq(site.id))
            .first(conn)?;
        let input_schema = InputSchema::parse(s.as_str(), site.deployment.clone())?;

        // Using the features field to store the instrument flag is a bit
        // backhanded, but since this will be used very rarely, should not
        // cause any headaches
        let instrument = features.iter().any(|s| s == "instrument");

        Ok(ManifestInfo {
            input_schema,
            description,
            repository,
            spec_version,
            instrument,
        })
    }
}

// Return how many blocks of history this subgraph should keep
pub fn history_blocks(conn: &PgConnection, site: &Site) -> Result<BlockNumber, StoreError> {
    use subgraph_manifest as sm;
    sm::table
        .select(sm::history_blocks)
        .filter(sm::id.eq(site.id))
        .first::<BlockNumber>(conn)
        .map_err(StoreError::from)
}

pub fn set_history_blocks(
    conn: &PgConnection,
    site: &Site,
    history_blocks: BlockNumber,
) -> Result<(), StoreError> {
    use subgraph_manifest as sm;

    update(sm::table.filter(sm::id.eq(site.id)))
        .set(sm::history_blocks.eq(history_blocks))
        .execute(conn)
        .map(|_| ())
        .map_err(StoreError::from)
}

#[allow(dead_code)]
pub fn features(conn: &PgConnection, site: &Site) -> Result<BTreeSet<SubgraphFeature>, StoreError> {
    use subgraph_manifest as sm;

    let features: Vec<String> = sm::table
        .select(sm::features)
        .filter(sm::id.eq(site.id))
        .first(conn)
        .unwrap();
    features
        .iter()
        .map(|f| SubgraphFeature::from_str(f).map_err(StoreError::from))
        .collect()
}

/// This migrates subgraphs that existed before the raw_yaml column was added.
pub fn set_manifest_raw_yaml(
    conn: &PgConnection,
    site: &Site,
    raw_yaml: &str,
) -> Result<(), StoreError> {
    use subgraph_manifest as sm;

    update(sm::table.filter(sm::id.eq(site.id)))
        .filter(sm::raw_yaml.is_null())
        .set(sm::raw_yaml.eq(raw_yaml))
        .execute(conn)
        .map(|_| ())
        .map_err(|e| e.into())
}

pub fn transact_block(
    conn: &PgConnection,
    site: &Site,
    ptr: &BlockPtr,
    firehose_cursor: &FirehoseCursor,
    count: i32,
) -> Result<BlockNumber, StoreError> {
    use crate::diesel::BoolExpressionMethods;
    use subgraph_deployment as d;

    // Work around a Diesel issue with serializing BigDecimals to numeric
    let number = format!("{}::numeric", ptr.number);

    let count_sql = entity_count_sql(count);

    let rows = update(
        d::table.filter(d::id.eq(site.id)).filter(
            // Asserts that the processing direction is forward.
            d::latest_ethereum_block_number
                .lt(sql(&number))
                .or(d::latest_ethereum_block_number.is_null()),
        ),
    )
    .set((
        d::latest_ethereum_block_number.eq(sql(&number)),
        d::latest_ethereum_block_hash.eq(ptr.hash_slice()),
        d::firehose_cursor.eq(firehose_cursor.as_ref()),
        d::entity_count.eq(sql(&count_sql)),
        d::current_reorg_depth.eq(0),
    ))
    .returning(d::earliest_block_number)
    .get_results::<BlockNumber>(conn)
    .map_err(StoreError::from)?;

    match rows.len() {
        // Common case: A single row was updated.
        1 => Ok(rows[0]),

        // No matching rows were found. This is an error. By the filter conditions, this can only be
        // due to a missing deployment (which `block_ptr` catches) or duplicate block processing.
        0 => match block_ptr(conn, &site.deployment)? {
            Some(block_ptr_from) if block_ptr_from.number >= ptr.number => Err(
                StoreError::DuplicateBlockProcessing(site.deployment.clone(), ptr.number),
            ),
            None | Some(_) => Err(StoreError::Unknown(anyhow!(
                "unknown error forwarding block ptr"
            ))),
        },

        // More than one matching row was found.
        _ => Err(StoreError::ConstraintViolation(
            "duplicate deployments in shard".to_owned(),
        )),
    }
}

pub fn forward_block_ptr(
    conn: &PgConnection,
    id: &DeploymentHash,
    ptr: &BlockPtr,
) -> Result<(), StoreError> {
    use crate::diesel::BoolExpressionMethods;
    use subgraph_deployment as d;

    // Work around a Diesel issue with serializing BigDecimals to numeric
    let number = format!("{}::numeric", ptr.number);

    let row_count = update(
        d::table.filter(d::deployment.eq(id.as_str())).filter(
            // Asserts that the processing direction is forward.
            d::latest_ethereum_block_number
                .lt(sql(&number))
                .or(d::latest_ethereum_block_number.is_null()),
        ),
    )
    .set((
        d::latest_ethereum_block_number.eq(sql(&number)),
        d::latest_ethereum_block_hash.eq(ptr.hash_slice()),
        d::current_reorg_depth.eq(0),
    ))
    .execute(conn)
    .map_err(StoreError::from)?;

    match row_count {
        // Common case: A single row was updated.
        1 => Ok(()),

        // No matching rows were found. This is an error. By the filter conditions, this can only be
        // due to a missing deployment (which `block_ptr` catches) or duplicate block processing.
        0 => match block_ptr(conn, id)? {
            Some(block_ptr_from) if block_ptr_from.number >= ptr.number => {
                Err(StoreError::DuplicateBlockProcessing(id.clone(), ptr.number))
            }
            None | Some(_) => Err(StoreError::Unknown(anyhow!(
                "unknown error forwarding block ptr"
            ))),
        },

        // More than one matching row was found.
        _ => Err(StoreError::ConstraintViolation(
            "duplicate deployments in shard".to_owned(),
        )),
    }
}

pub fn get_subgraph_firehose_cursor(
    conn: &PgConnection,
    site: Arc<Site>,
) -> Result<Option<String>, StoreError> {
    use subgraph_deployment as d;

    let res = d::table
        .filter(d::deployment.eq(site.deployment.as_str()))
        .select(d::firehose_cursor)
        .first::<Option<String>>(conn)
        .map_err(StoreError::from);
    res
}

pub fn revert_block_ptr(
    conn: &PgConnection,
    id: &DeploymentHash,
    ptr: BlockPtr,
    firehose_cursor: &FirehoseCursor,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    // Work around a Diesel issue with serializing BigDecimals to numeric
    let number = format!("{}::numeric", ptr.number);

    update(d::table.filter(d::deployment.eq(id.as_str())))
        .set((
            d::latest_ethereum_block_number.eq(sql(&number)),
            d::latest_ethereum_block_hash.eq(ptr.hash_slice()),
            d::firehose_cursor.eq(firehose_cursor.as_ref()),
            d::reorg_count.eq(d::reorg_count + 1),
            d::current_reorg_depth.eq(d::current_reorg_depth + 1),
            d::max_reorg_depth.eq(sql("greatest(current_reorg_depth + 1, max_reorg_depth)")),
        ))
        .execute(conn)
        .map(|_| ())
        .map_err(|e| e.into())
}

pub fn block_ptr(conn: &PgConnection, id: &DeploymentHash) -> Result<Option<BlockPtr>, StoreError> {
    use subgraph_deployment as d;

    let (number, hash) = d::table
        .filter(d::deployment.eq(id.as_str()))
        .select((
            d::latest_ethereum_block_number,
            d::latest_ethereum_block_hash,
        ))
        .first::<(Option<BigDecimal>, Option<Vec<u8>>)>(conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => StoreError::DeploymentNotFound(id.to_string()),
            e => e.into(),
        })?;

    let ptr = crate::detail::block(id.as_str(), "latest_ethereum_block", hash, number)?
        .map(|block| block.to_ptr());
    Ok(ptr)
}

/// Initialize the subgraph's block pointer. If the block pointer in
/// `latest_ethereum_block` is set already, do nothing. If it is still
/// `null`, set it to `start_ethereum_block` from `subgraph_manifest`
pub fn initialize_block_ptr(conn: &PgConnection, site: &Site) -> Result<(), StoreError> {
    use subgraph_deployment as d;
    use subgraph_manifest as m;

    let needs_init = d::table
        .filter(d::id.eq(site.id))
        .select(d::latest_ethereum_block_hash)
        .first::<Option<Vec<u8>>>(conn)
        .map_err(|e| {
            constraint_violation!(
                "deployment sgd{} must have been created before calling initialize_block_ptr but we got {}",
                site.id, e
            )
        })?
        .is_none();

    if needs_init {
        if let (Some(hash), Some(number)) = m::table
            .filter(m::id.eq(site.id))
            .select((m::start_block_hash, m::start_block_number))
            .first::<(Option<Vec<u8>>, Option<BlockNumber>)>(conn)?
        {
            let number = format!("{}::numeric", number);

            update(d::table.filter(d::id.eq(site.id)))
                .set((
                    d::latest_ethereum_block_hash.eq(&hash),
                    d::latest_ethereum_block_number.eq(sql(&number)),
                ))
                .execute(conn)
                .map(|_| ())
                .map_err(|e| e.into())
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

fn convert_to_u32(number: Option<i32>, field: &str, subgraph: &str) -> Result<u32, StoreError> {
    number
        .ok_or_else(|| constraint_violation!("missing {} for subgraph `{}`", field, subgraph))
        .and_then(|number| {
            u32::try_from(number).map_err(|_| {
                constraint_violation!(
                    "invalid value {:?} for {} in subgraph {}",
                    number,
                    field,
                    subgraph
                )
            })
        })
}

pub fn state(conn: &PgConnection, id: DeploymentHash) -> Result<DeploymentState, StoreError> {
    use subgraph_deployment as d;

    match d::table
        .filter(d::deployment.eq(id.as_str()))
        .select((
            d::deployment,
            d::reorg_count,
            d::max_reorg_depth,
            d::latest_ethereum_block_number,
            d::latest_ethereum_block_hash,
            d::earliest_block_number,
        ))
        .first::<(
            String,
            i32,
            i32,
            Option<BigDecimal>,
            Option<Vec<u8>>,
            BlockNumber,
        )>(conn)
        .optional()?
    {
        None => Err(StoreError::QueryExecutionError(format!(
            "No data found for subgraph {}",
            id
        ))),
        Some((
            _,
            reorg_count,
            max_reorg_depth,
            latest_block_number,
            latest_block_hash,
            earliest_block_number,
        )) => {
            let reorg_count = convert_to_u32(Some(reorg_count), "reorg_count", id.as_str())?;
            let max_reorg_depth =
                convert_to_u32(Some(max_reorg_depth), "max_reorg_depth", id.as_str())?;
            let latest_block = crate::detail::block(
                id.as_str(),
                "latest_block",
                latest_block_hash,
                latest_block_number,
            )?
            .ok_or_else(|| {
                StoreError::QueryExecutionError(format!(
                    "Subgraph `{}` has not started syncing yet. Wait for it to ingest \
                 a few blocks before querying it",
                    id
                ))
            })?
            .to_ptr();
            Ok(DeploymentState {
                id,
                reorg_count,
                max_reorg_depth,
                latest_block,
                earliest_block_number,
            })
        }
    }
}

/// Mark the deployment `id` as synced
pub fn set_synced(conn: &PgConnection, id: &DeploymentHash) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    update(
        d::table
            .filter(d::deployment.eq(id.as_str()))
            .filter(d::synced.eq(false)),
    )
    .set(d::synced.eq(true))
    .execute(conn)?;
    Ok(())
}

/// Returns `true` if the deployment (as identified by `site.id`)
pub fn exists(conn: &PgConnection, site: &Site) -> Result<bool, StoreError> {
    use subgraph_deployment as d;

    let exists = d::table
        .filter(d::id.eq(site.id))
        .count()
        .get_result::<i64>(conn)?
        > 0;
    Ok(exists)
}

/// Returns `true` if the deployment `id` exists and is synced
pub fn exists_and_synced(conn: &PgConnection, id: &str) -> Result<bool, StoreError> {
    use subgraph_deployment as d;

    let synced = d::table
        .filter(d::deployment.eq(id))
        .select(d::synced)
        .first(conn)
        .optional()?
        .unwrap_or(false);
    Ok(synced)
}

// Does nothing if the error already exists. Returns the error id.
fn insert_subgraph_error(conn: &PgConnection, error: &SubgraphError) -> anyhow::Result<String> {
    use subgraph_error as e;

    let error_id = hex::encode(stable_hash_legacy::utils::stable_hash::<SetHasher, _>(
        &error,
    ));
    let SubgraphError {
        subgraph_id,
        message,
        handler,
        block_ptr,
        deterministic,
    } = error;

    let block_num = match &block_ptr {
        None => {
            assert_eq!(*deterministic, false);
            crate::block_range::BLOCK_UNVERSIONED
        }
        Some(block) => crate::block_range::block_number(block),
    };

    insert_into(e::table)
        .values((
            e::id.eq(&error_id),
            e::subgraph_id.eq(subgraph_id.as_str()),
            e::message.eq(message),
            e::handler.eq(handler),
            e::deterministic.eq(deterministic),
            e::block_hash.eq(block_ptr.as_ref().map(|ptr| ptr.hash_slice())),
            e::block_range.eq((Bound::Included(block_num), Bound::Unbounded)),
        ))
        .on_conflict_do_nothing()
        .execute(conn)?;

    Ok(error_id)
}

pub fn fail(
    conn: &PgConnection,
    id: &DeploymentHash,
    error: &SubgraphError,
) -> Result<(), StoreError> {
    let error_id = insert_subgraph_error(conn, error)?;

    update_deployment_status(conn, id, SubgraphHealth::Failed, Some(error_id), None)?;

    Ok(())
}

pub fn update_non_fatal_errors(
    conn: &PgConnection,
    deployment_id: &DeploymentHash,
    health: SubgraphHealth,
    non_fatal_errors: Option<&[SubgraphError]>,
) -> Result<(), StoreError> {
    let error_ids = non_fatal_errors.map(|errors| {
        errors
            .iter()
            .map(|error| {
                hex::encode(stable_hash_legacy::utils::stable_hash::<SetHasher, _>(
                    error,
                ))
            })
            .collect::<Vec<_>>()
    });

    update_deployment_status(conn, deployment_id, health, None, error_ids)?;

    Ok(())
}

/// If `block` is `None`, assumes the latest block.
pub(crate) fn has_deterministic_errors(
    conn: &PgConnection,
    id: &DeploymentHash,
    block: BlockNumber,
) -> Result<bool, StoreError> {
    use subgraph_error as e;
    select(diesel::dsl::exists(
        e::table
            .filter(e::subgraph_id.eq(id.as_str()))
            .filter(e::deterministic)
            .filter(sql("block_range @> ").bind::<Integer, _>(block)),
    ))
    .get_result(conn)
    .map_err(|e| e.into())
}

pub fn update_deployment_status(
    conn: &PgConnection,
    deployment_id: &DeploymentHash,
    health: SubgraphHealth,
    fatal_error: Option<String>,
    non_fatal_errors: Option<Vec<String>>,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    update(d::table.filter(d::deployment.eq(deployment_id.as_str())))
        .set((
            d::failed.eq(health.is_failed()),
            d::health.eq(health),
            d::fatal_error.eq::<Option<String>>(fatal_error),
            d::non_fatal_errors.eq::<Vec<String>>(non_fatal_errors.unwrap_or(vec![])),
        ))
        .execute(conn)
        .map(|_| ())
        .map_err(StoreError::from)
}

/// Insert the errors and check if the subgraph needs to be set as
/// unhealthy. The `latest_block` is only used to check whether the subgraph
/// is healthy as of that block; errors are inserted according to the
/// `block_ptr` they contain
pub(crate) fn insert_subgraph_errors(
    conn: &PgConnection,
    id: &DeploymentHash,
    deterministic_errors: &[SubgraphError],
    latest_block: BlockNumber,
) -> Result<(), StoreError> {
    for error in deterministic_errors {
        insert_subgraph_error(conn, error)?;
    }

    check_health(conn, id, latest_block)
}

#[cfg(debug_assertions)]
pub(crate) fn error_count(conn: &PgConnection, id: &DeploymentHash) -> Result<usize, StoreError> {
    use subgraph_error as e;

    Ok(e::table
        .filter(e::subgraph_id.eq(id.as_str()))
        .count()
        .get_result::<i64>(conn)? as usize)
}

/// Checks if the subgraph is healthy or unhealthy as of the given block, or the subgraph latest
/// block if `None`, based on the presence of deterministic errors. Has no effect on failed subgraphs.
fn check_health(
    conn: &PgConnection,
    id: &DeploymentHash,
    block: BlockNumber,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    let has_errors = has_deterministic_errors(conn, id, block)?;

    let (new, old) = match has_errors {
        true => (SubgraphHealth::Unhealthy, SubgraphHealth::Healthy),
        false => (SubgraphHealth::Healthy, SubgraphHealth::Unhealthy),
    };

    update(
        d::table
            .filter(d::deployment.eq(id.as_str()))
            .filter(d::health.eq(old)),
    )
    .set(d::health.eq(new))
    .execute(conn)
    .map(|_| ())
    .map_err(|e| e.into())
}

pub(crate) fn health(conn: &PgConnection, id: DeploymentId) -> Result<SubgraphHealth, StoreError> {
    use subgraph_deployment as d;

    d::table
        .filter(d::id.eq(id))
        .select(d::health)
        .get_result(conn)
        .map_err(|e| e.into())
}

pub(crate) fn entities_with_causality_region(
    conn: &PgConnection,
    id: DeploymentId,
    schema: &InputSchema,
) -> Result<Vec<EntityType>, StoreError> {
    use subgraph_manifest as sm;

    sm::table
        .filter(sm::id.eq(id))
        .select(sm::entities_with_causality_region)
        .get_result::<Vec<String>>(conn)
        .map_err(|e| e.into())
        .map(|ents| {
            // It is possible to have entity types in
            // `entities_with_causality_region` that are not mentioned in
            // the schema.
            ents.into_iter()
                .filter_map(|ent| schema.entity_type(&ent).ok())
                .collect()
        })
}

/// Reverts the errors and updates the subgraph health if necessary.
pub(crate) fn revert_subgraph_errors(
    conn: &PgConnection,
    id: &DeploymentHash,
    reverted_block: BlockNumber,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;
    use subgraph_error as e;

    let lower_geq = format!("lower({}) >= ", BLOCK_RANGE_COLUMN);
    delete(
        e::table
            .filter(e::subgraph_id.eq(id.as_str()))
            .filter(sql(&lower_geq).bind::<Integer, _>(reverted_block)),
    )
    .execute(conn)?;

    // The result will be the same at `reverted_block` or `reverted_block - 1` since the errors at
    // `reverted_block` were just deleted, but semantically we care about `reverted_block - 1` which
    // is the block being reverted to.
    check_health(conn, id, reverted_block - 1)?;

    // If the deployment is failed in both `failed` and `status` columns,
    // update both values respectively to `false` and `healthy`. Basically
    // unfail the statuses.
    update(
        d::table
            .filter(d::deployment.eq(id.as_str()))
            .filter(d::failed.eq(true))
            .filter(d::health.eq(SubgraphHealth::Failed)),
    )
    .set((d::failed.eq(false), d::health.eq(SubgraphHealth::Healthy)))
    .execute(conn)
    .map(|_| ())
    .map_err(StoreError::from)
}

pub(crate) fn delete_error(conn: &PgConnection, error_id: &str) -> Result<(), StoreError> {
    use subgraph_error as e;
    delete(e::table.filter(e::id.eq(error_id)))
        .execute(conn)
        .map(|_| ())
        .map_err(StoreError::from)
}

/// Copy the dynamic data sources for `src` to `dst`. All data sources that
/// were created up to and including `target_block` will be copied.
pub(crate) fn copy_errors(
    conn: &PgConnection,
    src: &Site,
    dst: &Site,
    target_block: &BlockPtr,
) -> Result<usize, StoreError> {
    use subgraph_error as e;

    let src_nsp = ForeignServer::metadata_schema_in(&src.shard, &dst.shard);

    // Check whether there are any errors for dst which indicates we already
    // did copy
    let count = e::table
        .filter(e::subgraph_id.eq(dst.deployment.as_str()))
        .select(count(e::vid))
        .get_result::<i64>(conn)?;
    if count > 0 {
        return Ok(count as usize);
    }

    // We calculate a new id since the subgraph_error table has an exclusion
    // constraint on (id, block_range) and we need to make sure that newly
    // inserted errors do not collide with existing ones. For new subgraph
    // errors, we use a stable hash; for copied ones we just use an MD5.
    //
    // Longer term, we should get rid of the id column, since it is only
    // needed to deduplicate error messages, which would be better achieved
    // with a unique index
    let query = format!(
        "\
      insert into subgraphs.subgraph_error(id,
             subgraph_id, message, block_hash, handler, deterministic, block_range)
      select md5($2 || e.message || coalesce(e.block_hash, 'nohash') || coalesce(e.handler, 'nohandler') || e.deterministic) as id,
             $2 as subgraph_id, e.message, e.block_hash,
             e.handler, e.deterministic, e.block_range
        from {src_nsp}.subgraph_error e
       where e.subgraph_id = $1
         and lower(e.block_range) <= $3",
        src_nsp = src_nsp
    );

    Ok(sql_query(query)
        .bind::<Text, _>(src.deployment.as_str())
        .bind::<Text, _>(dst.deployment.as_str())
        .bind::<Integer, _>(target_block.number)
        .execute(conn)?)
}

/// Drop the schema `namespace`. This deletes all data for the subgraph, and
/// can not be reversed. It does not remove any of the metadata in the
/// `subgraphs` schema for the deployment.
///
/// Since long-running operations, like a vacuum on one of the tables in the
/// schema, could block dropping the schema indefinitely, this operation
/// will wait at most 2s to aquire all necessary locks, and fail if that is
/// not possible.
pub fn drop_schema(
    conn: &diesel::pg::PgConnection,
    namespace: &crate::primary::Namespace,
) -> Result<(), StoreError> {
    let query = format!(
        "set local lock_timeout=2000; drop schema if exists {} cascade",
        namespace
    );
    Ok(conn.batch_execute(&query)?)
}

pub fn drop_metadata(conn: &PgConnection, site: &Site) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    // We don't need to delete from subgraph_manifest or subgraph_error
    // since that cascades from deleting the subgraph_deployment
    delete(d::table.filter(d::id.eq(site.id))).execute(conn)?;
    Ok(())
}

pub fn create_deployment(
    conn: &PgConnection,
    site: &Site,
    deployment: DeploymentCreate,
    exists: bool,
    replace: bool,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;
    use subgraph_manifest as m;

    fn b(ptr: &Option<BlockPtr>) -> Option<&[u8]> {
        ptr.as_ref().map(|ptr| ptr.hash_slice())
    }

    fn n(ptr: &Option<BlockPtr>) -> SqlLiteral<Nullable<Numeric>> {
        match ptr {
            None => sql("null"),
            Some(ptr) => sql(&format!("{}::numeric", ptr.number)),
        }
    }

    let DeploymentCreate {
        manifest:
            SubgraphManifestEntity {
                spec_version,
                description,
                repository,
                features,
                schema,
                raw_yaml,
                entities_with_causality_region,
                history_blocks,
            },
        start_block,
        graft_base,
        graft_block,
        debug_fork,
        history_blocks: history_blocks_override,
    } = deployment;
    let earliest_block_number = start_block.as_ref().map(|ptr| ptr.number).unwrap_or(0);
    let entities_with_causality_region = Vec::from_iter(
        entities_with_causality_region
            .into_iter()
            .map(|et| et.as_str().to_owned()),
    );

    let deployment_values = (
        d::id.eq(site.id),
        d::deployment.eq(site.deployment.as_str()),
        d::failed.eq(false),
        d::synced.eq(false),
        d::health.eq(SubgraphHealth::Healthy),
        d::fatal_error.eq::<Option<String>>(None),
        d::non_fatal_errors.eq::<Vec<String>>(vec![]),
        d::earliest_block_number.eq(earliest_block_number),
        d::latest_ethereum_block_hash.eq(sql("null")),
        d::latest_ethereum_block_number.eq(sql("null")),
        d::entity_count.eq(sql("0")),
        d::graft_base.eq(graft_base.as_ref().map(|s| s.as_str())),
        d::graft_block_hash.eq(b(&graft_block)),
        d::graft_block_number.eq(n(&graft_block)),
        d::debug_fork.eq(debug_fork.as_ref().map(|s| s.as_str())),
    );

    let graph_node_version_id = GraphNodeVersion::create_or_get(conn)?;

    let manifest_values = (
        m::id.eq(site.id),
        m::spec_version.eq(spec_version),
        m::description.eq(description),
        m::repository.eq(repository),
        m::features.eq(features),
        m::schema.eq(schema),
        m::graph_node_version_id.eq(graph_node_version_id),
        // New subgraphs index only a prefix of bytea columns
        // see: attr-bytea-prefix
        m::use_bytea_prefix.eq(true),
        m::start_block_hash.eq(b(&start_block)),
        m::start_block_number.eq(start_block.as_ref().map(|ptr| ptr.number)),
        m::raw_yaml.eq(raw_yaml),
        m::entities_with_causality_region.eq(entities_with_causality_region),
        m::history_blocks.eq(history_blocks_override.unwrap_or(history_blocks)),
    );

    if exists && replace {
        update(d::table.filter(d::deployment.eq(site.deployment.as_str())))
            .set(deployment_values)
            .execute(conn)?;

        update(m::table.filter(m::id.eq(site.id)))
            .set(manifest_values)
            .execute(conn)?;
    } else {
        insert_into(d::table)
            .values(deployment_values)
            .execute(conn)?;

        insert_into(m::table)
            .values(manifest_values)
            .execute(conn)?;
    }
    Ok(())
}

fn entity_count_sql(count: i32) -> String {
    format!("entity_count + ({count})")
}

pub fn update_entity_count(conn: &PgConnection, site: &Site, count: i32) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    if count == 0 {
        return Ok(());
    }

    let count_sql = entity_count_sql(count);
    update(d::table.filter(d::id.eq(site.id)))
        .set(d::entity_count.eq(sql(&count_sql)))
        .execute(conn)?;
    Ok(())
}

/// Set the deployment's entity count to whatever `full_count_query` produces
pub fn set_entity_count(
    conn: &PgConnection,
    site: &Site,
    full_count_query: &str,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    let full_count_query = format!("({})", full_count_query);
    update(d::table.filter(d::id.eq(site.id)))
        .set(d::entity_count.eq(sql(&full_count_query)))
        .execute(conn)?;
    Ok(())
}

/// Set the earliest block of `site` to the larger of `earliest_block` and
/// the current value. This means that the `earliest_block_number` can never
/// go backwards, only forward. This is important so that copying into
/// `site` can not move the earliest block backwards if `site` was also
/// pruned while the copy was running.
pub fn set_earliest_block(
    conn: &PgConnection,
    site: &Site,
    earliest_block: BlockNumber,
) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    update(d::table.filter(d::id.eq(site.id)))
        .set(d::earliest_block_number.eq(earliest_block))
        .filter(d::earliest_block_number.lt(earliest_block))
        .execute(conn)?;
    Ok(())
}

/// Copy the `earliest_block` attribute from `src` to `dst`. The copy might
/// go across shards and use the metadata tables mapped into the shard for
/// `conn` which must be the shard for `dst`
pub fn copy_earliest_block(conn: &PgConnection, src: &Site, dst: &Site) -> Result<(), StoreError> {
    use subgraph_deployment as d;

    let src_nsp = ForeignServer::metadata_schema_in(&src.shard, &dst.shard);

    let query = format!(
        "(select earliest_block_number from {src_nsp}.subgraph_deployment where id = {})",
        src.id
    );

    update(d::table.filter(d::id.eq(dst.id)))
        .set(d::earliest_block_number.eq(sql(&query)))
        .execute(conn)?;

    Ok(())
}

pub fn on_sync(conn: &PgConnection, id: impl Into<DeploymentId>) -> Result<OnSync, StoreError> {
    use subgraph_manifest as m;

    let s = m::table
        .filter(m::id.eq(id.into()))
        .select(m::on_sync)
        .get_result::<Option<String>>(conn)?;
    OnSync::try_from(s.as_deref())
}

pub fn set_on_sync(conn: &PgConnection, site: &Site, on_sync: OnSync) -> Result<(), StoreError> {
    use subgraph_manifest as m;

    let n = update(m::table.filter(m::id.eq(site.id)))
        .set(m::on_sync.eq(on_sync.to_sql()))
        .execute(conn)?;

    match n {
        0 => Err(StoreError::DeploymentNotFound(site.to_string())),
        1 => Ok(()),
        _ => Err(constraint_violation!(
            "multiple manifests for deployment {}",
            site.to_string()
        )),
    }
}

/// Lock the deployment `site` for writes while `f` is running. The lock can
/// cross transactions, and `f` can therefore execute multiple transactions
/// while other write activity for that deployment is locked out. Block the
/// current thread until we can acquire the lock.
//  see also: deployment-lock-for-update
pub fn with_lock<F, R>(conn: &PgConnection, site: &Site, f: F) -> Result<R, StoreError>
where
    F: FnOnce() -> Result<R, StoreError>,
{
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(15));
    while !advisory_lock::lock_deployment_session(conn, site)? {
        backoff.sleep();
    }
    let res = f();
    advisory_lock::unlock_deployment_session(conn, site)?;
    res
}
