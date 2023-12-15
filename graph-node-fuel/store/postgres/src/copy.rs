//! This module if repsonsible for copying the data of an existing subgraph
//! `src` to a new subgraph `dst`. The copy is done in batches so that
//! copying does not cause long-running transactions since they lead to
//! table bloat in the rest of the system.
//!
//! The `TARGET_DURATION` defines how long any individual copy operation
//! should take at most, and the code adapts the number of entities that are
//! copied in each batch to be close to that time span.
//!
//! The progress of copy operations is recorded in the tables
//! `subgraphs.copy_state` and `subgraphs.copy_table_state` so that a copy
//! operation can resume after an interruption, for example, because
//! `graph-node` was restarted while the copy was running.
use std::{
    convert::TryFrom,
    io::Write,
    sync::Arc,
    time::{Duration, Instant},
};

use diesel::{
    dsl::sql,
    insert_into,
    pg::Pg,
    r2d2::{ConnectionManager, PooledConnection},
    select,
    serialize::Output,
    sql_query,
    sql_types::{BigInt, Integer},
    types::{FromSql, ToSql},
    update, Connection as _, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl,
};
use graph::{
    constraint_violation,
    prelude::{info, o, warn, BlockNumber, BlockPtr, Logger, StoreError, ENV_VARS},
    schema::EntityType,
};

use crate::{
    advisory_lock, catalog,
    dynds::DataSourcesTable,
    primary::{DeploymentId, Site},
};
use crate::{connection_pool::ConnectionPool, relational::Layout};
use crate::{relational::Table, relational_queries as rq};

/// The initial batch size for tables that do not have an array column
const INITIAL_BATCH_SIZE: i64 = 10_000;
/// The initial batch size for tables that do have an array column; those
/// arrays can be large and large arrays will slow down copying a lot. We
/// therefore tread lightly in that case
const INITIAL_BATCH_SIZE_LIST: i64 = 100;

const LOG_INTERVAL: Duration = Duration::from_secs(3 * 60);

/// If replicas are lagging by more than this, the copying code will pause
/// for a while to allow replicas to catch up
const MAX_REPLICATION_LAG: Duration = Duration::from_secs(60);
/// If replicas need to catch up, do not resume copying until the lag is
/// less than this
const ACCEPTABLE_REPLICATION_LAG: Duration = Duration::from_secs(30);
/// When replicas are lagging too much, sleep for this long before checking
/// the lag again
const REPLICATION_SLEEP: Duration = Duration::from_secs(10);

table! {
    subgraphs.copy_state(dst) {
        // deployment_schemas.id
        src -> Integer,
        // deployment_schemas.id
        dst -> Integer,
        target_block_hash -> Binary,
        target_block_number -> Integer,
        started_at -> Timestamptz,
        finished_at -> Nullable<Timestamptz>,
        cancelled_at -> Nullable<Timestamptz>,
    }
}

table! {
    subgraphs.copy_table_state {
        id -> Integer,
        entity_type -> Text,
        // references copy_state.dst
        dst -> Integer,
        next_vid -> BigInt,
        target_vid -> BigInt,
        batch_size -> BigInt,
        started_at -> Timestamptz,
        finished_at -> Nullable<Timestamptz>,
        // Measures just the time we spent working, not any wait time for
        // connections or the like
        duration_ms -> BigInt,
    }
}

// This is the same as primary::active_copies, but mapped into each shard
table! {
    primary_public.active_copies(dst) {
        src -> Integer,
        dst -> Integer,
        cancelled_at -> Nullable<Date>,
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Status {
    Finished,
    Cancelled,
}

#[allow(dead_code)]
struct CopyState {
    src: Arc<Layout>,
    dst: Arc<Layout>,
    target_block: BlockPtr,
    tables: Vec<TableState>,
}

impl CopyState {
    fn new(
        conn: &PgConnection,
        src: Arc<Layout>,
        dst: Arc<Layout>,
        target_block: BlockPtr,
    ) -> Result<CopyState, StoreError> {
        use copy_state as cs;

        let crosses_shards = dst.site.shard != src.site.shard;
        if crosses_shards {
            src.import_schema(conn)?;
        }

        let state = match cs::table
            .filter(cs::dst.eq(dst.site.id))
            .select((cs::src, cs::target_block_hash, cs::target_block_number))
            .first::<(DeploymentId, Vec<u8>, BlockNumber)>(conn)
            .optional()?
        {
            Some((src_id, hash, number)) => {
                let stored_target_block = BlockPtr::from((hash, number));
                if stored_target_block != target_block {
                    return Err(constraint_violation!(
                        "CopyState {} for copying {} to {} has incompatible block pointer {} instead of {}",
                        dst.site.id,
                        src.site.deployment,
                        dst.site.deployment,
                        stored_target_block,
                        target_block));
                }
                if src_id != src.site.id {
                    return Err(constraint_violation!(
                        "CopyState {} for copying {} to {} has incompatible source {} instead of {}",
                        dst.site.id,
                        src.site.deployment,
                        dst.site.deployment,
                        src_id,
                        src.site.id
                    ));
                }
                Self::load(conn, src, dst, target_block)
            }
            None => Self::create(conn, src, dst, target_block),
        }?;

        Ok(state)
    }

    fn load(
        conn: &PgConnection,
        src: Arc<Layout>,
        dst: Arc<Layout>,
        target_block: BlockPtr,
    ) -> Result<CopyState, StoreError> {
        let tables = TableState::load(conn, src.as_ref(), dst.as_ref())?;
        Ok(CopyState {
            src,
            dst,
            target_block,
            tables,
        })
    }

    fn create(
        conn: &PgConnection,
        src: Arc<Layout>,
        dst: Arc<Layout>,
        target_block: BlockPtr,
    ) -> Result<CopyState, StoreError> {
        use copy_state as cs;
        use copy_table_state as cts;

        insert_into(cs::table)
            .values((
                cs::src.eq(src.site.id),
                cs::dst.eq(dst.site.id),
                cs::target_block_hash.eq(target_block.hash_slice()),
                cs::target_block_number.eq(target_block.number),
            ))
            .execute(conn)?;

        let mut tables: Vec<_> = dst
            .tables
            .values()
            .filter_map(|dst_table| {
                src.table_for_entity(&dst_table.object)
                    .ok()
                    .map(|src_table| {
                        TableState::init(
                            conn,
                            dst.site.clone(),
                            src_table.clone(),
                            dst_table.clone(),
                            &target_block,
                        )
                    })
            })
            .collect::<Result<_, _>>()?;
        tables.sort_by_key(|table| table.batch.dst.object.to_string());

        let values = tables
            .iter()
            .map(|table| {
                (
                    cts::entity_type.eq(table.batch.dst.object.as_str()),
                    cts::dst.eq(dst.site.id),
                    cts::next_vid.eq(table.batch.next_vid),
                    cts::target_vid.eq(table.batch.target_vid),
                    cts::batch_size.eq(table.batch.batch_size.size),
                )
            })
            .collect::<Vec<_>>();
        insert_into(cts::table).values(values).execute(conn)?;

        Ok(CopyState {
            src,
            dst,
            target_block,
            tables,
        })
    }

    fn crosses_shards(&self) -> bool {
        self.dst.site.shard != self.src.site.shard
    }

    fn finished(&self, conn: &PgConnection) -> Result<(), StoreError> {
        use copy_state as cs;

        update(cs::table.filter(cs::dst.eq(self.dst.site.id)))
            .set(cs::finished_at.eq(sql("now()")))
            .execute(conn)?;

        // If we imported the schema for `src`, and no other in-progress
        // copy is using it, get rid of it again
        if self.crosses_shards() {
            let has_active_copies = select(diesel::dsl::exists(
                cs::table
                    .filter(cs::src.eq(self.src.site.id))
                    .filter(cs::finished_at.is_null()),
            ))
            .get_result::<bool>(conn)?;
            if !has_active_copies {
                // This is a foreign schema that nobody is using anymore,
                // get rid of it. As a safety check (on top of the one that
                // drop_foreign_schema does), see that we do not have
                // metadata for `src`
                if crate::deployment::exists(conn, &self.src.site)? {
                    return Err(constraint_violation!(
                        "we think we are copying {}[{}] across shards from {} to {}, but the \
                        source subgraph is actually in this shard",
                        self.src.site.deployment,
                        self.src.site.id,
                        self.src.site.shard,
                        self.dst.site.shard
                    ));
                }
                crate::catalog::drop_foreign_schema(conn, self.src.site.as_ref())?;
            }
        }
        Ok(())
    }
}

pub(crate) fn source(conn: &PgConnection, dst: &Site) -> Result<Option<DeploymentId>, StoreError> {
    use copy_state as cs;

    cs::table
        .filter(cs::dst.eq(dst.id))
        .select(cs::src)
        .get_result::<DeploymentId>(conn)
        .optional()
        .map_err(StoreError::from)
}

/// Track the desired size of a batch in such a way that doing the next
/// batch gets close to TARGET_DURATION for the time it takes to copy one
/// batch, but don't step up the size by more than 2x at once
#[derive(Debug, Queryable)]
pub(crate) struct AdaptiveBatchSize {
    pub size: i64,
}

impl AdaptiveBatchSize {
    pub fn new(table: &Table) -> Self {
        let size = if table.columns.iter().any(|col| col.is_list()) {
            INITIAL_BATCH_SIZE_LIST
        } else {
            INITIAL_BATCH_SIZE
        };

        Self { size }
    }

    // adjust batch size by trying to extrapolate in such a way that we
    // get close to TARGET_DURATION for the time it takes to copy one
    // batch, but don't step up batch_size by more than 2x at once
    pub fn adapt(&mut self, duration: Duration) {
        // Avoid division by zero
        let duration = duration.as_millis().max(1);
        let new_batch_size = self.size as f64
            * ENV_VARS.store.batch_target_duration.as_millis() as f64
            / duration as f64;
        self.size = (2 * self.size).min(new_batch_size.round() as i64);
    }
}

impl ToSql<BigInt, Pg> for AdaptiveBatchSize {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> diesel::serialize::Result {
        <i64 as ToSql<BigInt, Pg>>::to_sql(&self.size, out)
    }
}

impl FromSql<BigInt, Pg> for AdaptiveBatchSize {
    fn from_sql(bytes: Option<&[u8]>) -> diesel::deserialize::Result<Self> {
        let size = <i64 as FromSql<BigInt, Pg>>::from_sql(bytes)?;
        Ok(AdaptiveBatchSize { size })
    }
}

/// A helper to copy entities from one table to another in batches that are
/// small enough to not interfere with the rest of the operations happening
/// in the database. The `src` and `dst` table must have the same structure
/// so that we can copy rows from one to the other with very little
/// transformation. See `CopyEntityBatchQuery` for the details of what
/// exactly that means
pub(crate) struct BatchCopy {
    src: Arc<Table>,
    dst: Arc<Table>,
    /// The `vid` of the next entity version that we will copy
    next_vid: i64,
    /// The last `vid` that should be copied
    target_vid: i64,
    batch_size: AdaptiveBatchSize,
}

impl BatchCopy {
    pub fn new(src: Arc<Table>, dst: Arc<Table>, first_vid: i64, last_vid: i64) -> Self {
        let batch_size = AdaptiveBatchSize::new(&dst);

        Self {
            src,
            dst,
            next_vid: first_vid,
            target_vid: last_vid,
            batch_size,
        }
    }

    /// Copy one batch of entities and update internal state so that the
    /// next call to `run` will copy the next batch
    pub fn run(&mut self, conn: &PgConnection) -> Result<Duration, StoreError> {
        let start = Instant::now();

        // Copy all versions with next_vid <= vid <= next_vid + batch_size - 1,
        // but do not go over target_vid
        let last_vid = (self.next_vid + self.batch_size.size - 1).min(self.target_vid);
        rq::CopyEntityBatchQuery::new(self.dst.as_ref(), &self.src, self.next_vid, last_vid)?
            .execute(conn)?;

        let duration = start.elapsed();

        // remember how far we got
        self.next_vid = last_vid + 1;

        self.batch_size.adapt(duration);

        Ok(duration)
    }

    pub fn finished(&self) -> bool {
        self.next_vid > self.target_vid
    }
}

struct TableState {
    batch: BatchCopy,
    dst_site: Arc<Site>,
    duration_ms: i64,
}

impl TableState {
    fn init(
        conn: &PgConnection,
        dst_site: Arc<Site>,
        src: Arc<Table>,
        dst: Arc<Table>,
        target_block: &BlockPtr,
    ) -> Result<Self, StoreError> {
        #[derive(QueryableByName)]
        struct MaxVid {
            #[sql_type = "diesel::sql_types::BigInt"]
            max_vid: i64,
        }

        let max_block_clause = if src.immutable {
            "block$ <= $1"
        } else {
            "lower(block_range) <= $1"
        };
        let target_vid = sql_query(format!(
            "select coalesce(max(vid), -1) as max_vid from {} where {}",
            src.qualified_name.as_str(),
            max_block_clause
        ))
        .bind::<Integer, _>(&target_block.number)
        .load::<MaxVid>(conn)?
        .first()
        .map(|v| v.max_vid)
        .unwrap_or(-1);

        Ok(Self {
            batch: BatchCopy::new(src, dst, 0, target_vid),
            dst_site,
            duration_ms: 0,
        })
    }

    fn finished(&self) -> bool {
        self.batch.finished()
    }

    fn load(
        conn: &PgConnection,
        src_layout: &Layout,
        dst_layout: &Layout,
    ) -> Result<Vec<TableState>, StoreError> {
        use copy_table_state as cts;

        fn resolve_entity(
            layout: &Layout,
            kind: &str,
            entity_type: &EntityType,
            dst: DeploymentId,
            id: i32,
        ) -> Result<Arc<Table>, StoreError> {
            layout
                .table_for_entity(entity_type)
                .map_err(|e| {
                    constraint_violation!(
                        "invalid {} table {} in CopyState {} (table {}): {}",
                        kind,
                        entity_type,
                        dst,
                        id,
                        e
                    )
                })
                .map(|table| table.clone())
        }

        cts::table
            .filter(cts::dst.eq(dst_layout.site.id))
            .select((
                cts::id,
                cts::entity_type,
                cts::next_vid,
                cts::target_vid,
                cts::batch_size,
                cts::duration_ms,
            ))
            .order_by(cts::entity_type)
            .load::<(i32, String, i64, i64, i64, i64)>(conn)?
            .into_iter()
            .map(
                |(id, entity_type, current_vid, target_vid, size, duration_ms)| {
                    let entity_type = src_layout.input_schema.entity_type(&entity_type)?;
                    let src =
                        resolve_entity(src_layout, "source", &entity_type, dst_layout.site.id, id);
                    let dst = resolve_entity(
                        dst_layout,
                        "destination",
                        &entity_type,
                        dst_layout.site.id,
                        id,
                    );
                    match (src, dst) {
                        (Ok(src), Ok(dst)) => {
                            let mut batch = BatchCopy::new(src, dst, current_vid, target_vid);
                            let batch_size = AdaptiveBatchSize { size };

                            batch.batch_size = batch_size;

                            Ok(TableState {
                                batch,
                                dst_site: dst_layout.site.clone(),
                                duration_ms,
                            })
                        }
                        (Err(e), _) => Err(e),
                        (_, Err(e)) => Err(e),
                    }
                },
            )
            .collect()
    }

    fn record_progress(
        &mut self,
        conn: &PgConnection,
        elapsed: Duration,
        first_batch: bool,
    ) -> Result<(), StoreError> {
        use copy_table_state as cts;

        // This conversion will become a problem if a copy takes longer than
        // 300B years
        self.duration_ms += i64::try_from(elapsed.as_millis()).unwrap_or(0);

        if first_batch {
            // Reset started_at so that finished_at - started_at is an
            // accurate indication of how long we worked on a table.
            update(
                cts::table
                    .filter(cts::dst.eq(self.dst_site.id))
                    .filter(cts::entity_type.eq(self.batch.dst.object.as_str())),
            )
            .set(cts::started_at.eq(sql("now()")))
            .execute(conn)?;
        }
        let values = (
            cts::next_vid.eq(self.batch.next_vid),
            cts::batch_size.eq(self.batch.batch_size.size),
            cts::duration_ms.eq(self.duration_ms),
        );
        update(
            cts::table
                .filter(cts::dst.eq(self.dst_site.id))
                .filter(cts::entity_type.eq(self.batch.dst.object.as_str())),
        )
        .set(values)
        .execute(conn)?;
        Ok(())
    }

    fn record_finished(&self, conn: &PgConnection) -> Result<(), StoreError> {
        use copy_table_state as cts;

        update(
            cts::table
                .filter(cts::dst.eq(self.dst_site.id))
                .filter(cts::entity_type.eq(self.batch.dst.object.as_str())),
        )
        .set(cts::finished_at.eq(sql("now()")))
        .execute(conn)?;
        Ok(())
    }

    fn is_cancelled(&self, conn: &PgConnection) -> Result<bool, StoreError> {
        use active_copies as ac;

        let dst = self.dst_site.as_ref();
        let canceled = ac::table
            .filter(ac::dst.eq(dst.id))
            .select(ac::cancelled_at.is_not_null())
            .get_result::<bool>(conn)?;
        if canceled {
            use copy_state as cs;

            update(cs::table.filter(cs::dst.eq(dst.id)))
                .set(cs::cancelled_at.eq(sql("now()")))
                .execute(conn)?;
        }
        Ok(canceled)
    }

    fn copy_batch(&mut self, conn: &PgConnection) -> Result<Status, StoreError> {
        let first_batch = self.batch.next_vid == 0;

        let duration = self.batch.run(conn)?;

        self.record_progress(conn, duration, first_batch)?;

        if self.finished() {
            self.record_finished(conn)?;
        }

        Ok(Status::Finished)
    }
}

// A helper for logging progress while data is being copied
struct CopyProgress<'a> {
    logger: &'a Logger,
    last_log: Instant,
    src: Arc<Site>,
    dst: Arc<Site>,
    current_vid: i64,
    target_vid: i64,
}

impl<'a> CopyProgress<'a> {
    fn new(logger: &'a Logger, state: &CopyState) -> Self {
        let target_vid: i64 = state
            .tables
            .iter()
            .map(|table| table.batch.target_vid)
            .sum();
        Self {
            logger,
            last_log: Instant::now(),
            src: state.src.site.clone(),
            dst: state.dst.site.clone(),
            current_vid: 0,
            target_vid,
        }
    }

    fn start(&self) {
        info!(
            self.logger,
            "Initialize data copy from {}[{}] to {}[{}]",
            self.src.deployment,
            self.src.namespace,
            self.dst.deployment,
            self.dst.namespace
        );
    }

    fn progress_pct(current_vid: i64, target_vid: i64) -> f64 {
        // When a step is done, current_vid == target_vid + 1; don't report
        // more than 100% completion
        if target_vid == 0 || current_vid >= target_vid {
            100.0
        } else {
            current_vid as f64 / target_vid as f64 * 100.0
        }
    }

    fn update(&mut self, batch: &BatchCopy) {
        if self.last_log.elapsed() > LOG_INTERVAL {
            info!(
                self.logger,
                "Copied {:.2}% of `{}` entities ({}/{} entity versions), {:.2}% of overall data",
                Self::progress_pct(batch.next_vid, batch.target_vid),
                batch.dst.object,
                batch.next_vid,
                batch.target_vid,
                Self::progress_pct(self.current_vid + batch.next_vid, self.target_vid)
            );
            self.last_log = Instant::now();
        }
    }

    fn table_finished(&mut self, batch: &BatchCopy) {
        self.current_vid += batch.next_vid;
    }

    fn finished(&self) {
        info!(
            self.logger,
            "Finished copying data into {}[{}]", self.dst.deployment, self.dst.namespace
        );
    }
}

/// A helper for copying subgraphs
pub struct Connection {
    /// The connection pool for the shard that will contain the destination
    /// of the copy
    logger: Logger,
    conn: PooledConnection<ConnectionManager<PgConnection>>,
    src: Arc<Layout>,
    dst: Arc<Layout>,
    target_block: BlockPtr,
    src_manifest_idx_and_name: Vec<(i32, String)>,
    dst_manifest_idx_and_name: Vec<(i32, String)>,
}

impl Connection {
    /// Create a new copy connection. It takes a connection from the
    /// dedicated fdw pool in `pool`, and releases it only after the entire
    /// copy has finished, which might take hours or even days. This method
    /// will block until it was able to get a fdw connection. The overall
    /// effect is that new copy requests will not start until a connection
    /// is available.
    pub fn new(
        logger: &Logger,
        pool: ConnectionPool,
        src: Arc<Layout>,
        dst: Arc<Layout>,
        target_block: BlockPtr,
        src_manifest_idx_and_name: Vec<(i32, String)>,
        dst_manifest_idx_and_name: Vec<(i32, String)>,
    ) -> Result<Self, StoreError> {
        let logger = logger.new(o!("dst" => dst.site.namespace.to_string()));

        if src.site.schema_version != dst.site.schema_version {
            return Err(StoreError::ConstraintViolation(format!(
                "attempted to copy between different schema versions, \
                 source version is {} but destination version is {}",
                src.site.schema_version, dst.site.schema_version
            )));
        }

        let mut last_log = Instant::now();
        let conn = pool.get_fdw(&logger, || {
            if last_log.elapsed() > LOG_INTERVAL {
                info!(&logger, "waiting for other copy operations to finish");
                last_log = Instant::now();
            }
            false
        })?;
        Ok(Self {
            logger,
            conn,
            src,
            dst,
            target_block,
            src_manifest_idx_and_name,
            dst_manifest_idx_and_name,
        })
    }

    fn transaction<T, F>(&self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&PgConnection) -> Result<T, StoreError>,
    {
        self.conn.transaction(|| f(&self.conn))
    }

    fn copy_private_data_sources(&self, state: &CopyState) -> Result<(), StoreError> {
        if state.src.site.schema_version.private_data_sources() {
            DataSourcesTable::new(state.src.site.namespace.clone()).copy_to(
                &self.conn,
                &DataSourcesTable::new(state.dst.site.namespace.clone()),
                state.target_block.number,
                &self.src_manifest_idx_and_name,
                &self.dst_manifest_idx_and_name,
            )?;
        }
        Ok(())
    }

    pub fn copy_data_internal(&self) -> Result<Status, StoreError> {
        let mut state = self.transaction(|conn| {
            CopyState::new(
                conn,
                self.src.clone(),
                self.dst.clone(),
                self.target_block.clone(),
            )
        })?;

        let mut progress = CopyProgress::new(&self.logger, &state);
        progress.start();

        for table in state.tables.iter_mut().filter(|table| !table.finished()) {
            while !table.finished() {
                // It is important that this check happens outside the write
                // transaction so that we do not hold on to locks acquired
                // by the check
                if table.is_cancelled(&self.conn)? {
                    return Ok(Status::Cancelled);
                }

                // Pause copying if replication is lagging behind to avoid
                // overloading replicas
                let mut lag = catalog::replication_lag(&self.conn)?;
                if lag > MAX_REPLICATION_LAG {
                    loop {
                        info!(&self.logger,
                             "Replicas are lagging too much; pausing copying for {}s to allow them to catch up",
                             REPLICATION_SLEEP.as_secs();
                             "lag_s" => lag.as_secs());
                        std::thread::sleep(REPLICATION_SLEEP);
                        lag = catalog::replication_lag(&self.conn)?;
                        if lag <= ACCEPTABLE_REPLICATION_LAG {
                            break;
                        }
                    }
                }

                let status = self.transaction(|conn| table.copy_batch(conn))?;
                if status == Status::Cancelled {
                    return Ok(status);
                }
                progress.update(&table.batch);
            }
            progress.table_finished(&table.batch);
        }

        self.copy_private_data_sources(&state)?;

        self.transaction(|conn| state.finished(conn))?;
        progress.finished();

        Ok(Status::Finished)
    }

    /// Copy the data for the subgraph `src` to the subgraph `dst`. The
    /// schema for both subgraphs must have already been set up. The
    /// `target_block` must be far enough behind the chain head so that the
    /// block is guaranteed to not be subject to chain reorgs. All data up
    /// to and including `target_block` will be copied.
    ///
    /// The copy logic makes heavy use of the fact that the `vid` and
    /// `block_range` of entity versions are related since for two entity
    /// versions `v1` and `v2` such that `v1.vid <= v2.vid`, we know that
    /// `lower(v1.block_range) <= lower(v2.block_range)`. Conversely,
    /// therefore, we have that `lower(v2.block_range) >
    /// lower(v1.block_range) => v2.vid > v1.vid` and we can therefore stop
    /// the copying of each table as soon as we hit `max_vid = max { v.vid |
    /// lower(v.block_range) <= target_block.number }`.
    pub fn copy_data(&self) -> Result<Status, StoreError> {
        // We require sole access to the destination site, and that we get a
        // consistent view of what has been copied so far. In general, that
        // is always true. It can happen though that this function runs when
        // a query for copying from a previous run of graph-node is still in
        // progress. Since each transaction during copying takes a very long
        // time, it might take several minutes for Postgres to notice that
        // the old process has been terminated. The locking strategy here
        // ensures that we wait until that has happened.
        info!(
            &self.logger,
            "Obtaining copy lock (this might take a long time if another process is still copying)"
        );
        advisory_lock::lock_copying(&self.conn, self.dst.site.as_ref())?;
        let res = self.copy_data_internal();
        advisory_lock::unlock_copying(&self.conn, self.dst.site.as_ref())?;
        if matches!(res, Ok(Status::Cancelled)) {
            warn!(&self.logger, "Copying was cancelled and is incomplete");
        }
        res
    }
}
