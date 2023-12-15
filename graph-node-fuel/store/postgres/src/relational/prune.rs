use std::{fmt::Write, sync::Arc, time::Instant};

use diesel::{
    connection::SimpleConnection,
    sql_query,
    sql_types::{BigInt, Integer},
    Connection, PgConnection, RunQueryDsl,
};
use graph::{
    components::store::{PrunePhase, PruneReporter, PruneRequest, PruningStrategy, VersionStats},
    prelude::{
        BlockNumber, CancelHandle, CancelToken, CancelableError, CheapClone, StoreError,
        BLOCK_NUMBER_MAX,
    },
    slog::{warn, Logger},
};
use itertools::Itertools;

use crate::{
    catalog,
    copy::AdaptiveBatchSize,
    deployment,
    relational::{Table, VID_COLUMN},
};

use super::{Catalog, Layout, Namespace};

// Additions to `Table` that are useful for pruning
impl Table {
    /// Return the first and last vid of any entity that is visible in the
    /// block range from `first_block` (inclusive) to `last_block`
    /// (exclusive)
    fn vid_range(
        &self,
        conn: &PgConnection,
        first_block: BlockNumber,
        last_block: BlockNumber,
    ) -> Result<(i64, i64), StoreError> {
        #[derive(QueryableByName)]
        struct VidRange {
            #[sql_type = "BigInt"]
            min_vid: i64,
            #[sql_type = "BigInt"]
            max_vid: i64,
        }

        // Determine the last vid that we need to copy
        let VidRange { min_vid, max_vid } = sql_query(format!(
            "/* controller=prune,first={first_block},last={last_block} */ \
                     select coalesce(min(vid), 0) as min_vid, \
                            coalesce(max(vid), -1) as max_vid from {src} \
                      where lower(block_range) <= $2 \
                        and coalesce(upper(block_range), 2147483647) > $1 \
                        and coalesce(upper(block_range), 2147483647) <= $2 \
                        and block_range && int4range($1, $2)",
            src = self.qualified_name,
        ))
        .bind::<Integer, _>(first_block)
        .bind::<Integer, _>(last_block)
        .get_result::<VidRange>(conn)?;
        Ok((min_vid, max_vid))
    }
}

/// Utility to copy relevant data out of a source table and into a new
/// destination table and replace the source table with the destination
/// table
struct TablePair {
    // The original unpruned table
    src: Arc<Table>,
    // The temporary table to which we copy the data we'd like to keep. It
    // has the same name as `src` but is in a different namespace
    dst: Arc<Table>,
    src_nsp: Namespace,
    dst_nsp: Namespace,
}

impl TablePair {
    /// Create a `TablePair` for `src`. This creates a new table `dst` with
    /// the same structure as the `src` table in the database, but in a
    /// different namespace so that the names of indexes etc. don't clash
    fn create(
        conn: &PgConnection,
        src: Arc<Table>,
        src_nsp: Namespace,
        dst_nsp: Namespace,
        catalog: &Catalog,
    ) -> Result<Self, StoreError> {
        let dst = src.new_like(&dst_nsp, &src.name);

        let mut query = String::new();
        if catalog::table_exists(conn, dst_nsp.as_str(), &dst.name)? {
            writeln!(query, "truncate table {};", dst.qualified_name)?;
        } else {
            dst.as_ddl(catalog, &mut query)?;
        }
        conn.batch_execute(&query)?;

        Ok(TablePair {
            src,
            dst,
            src_nsp,
            dst_nsp,
        })
    }

    /// Copy all entity versions visible between `earliest_block` and
    /// `final_block` in batches, where each batch is a separate
    /// transaction. Write activity for nonfinal blocks can happen
    /// concurrently to this copy
    fn copy_final_entities(
        &self,
        conn: &PgConnection,
        reporter: &mut dyn PruneReporter,
        earliest_block: BlockNumber,
        final_block: BlockNumber,
        cancel: &CancelHandle,
    ) -> Result<(), CancelableError<StoreError>> {
        let column_list = self.column_list();

        // Determine the last vid that we need to copy
        let (min_vid, max_vid) = self.src.vid_range(conn, earliest_block, final_block)?;

        let mut batch_size = AdaptiveBatchSize::new(&self.src);
        // The first vid we still need to copy
        let mut next_vid = min_vid;
        while next_vid <= max_vid {
            let start = Instant::now();
            let rows = conn.transaction(|| {
                // Page through all rows in `src` in batches of `batch_size`
                // and copy the ones that are visible to queries at block
                // heights between `earliest_block` and `final_block`, but
                // whose block_range does not extend past `final_block`
                // since they could still be reverted while we copy.
                // The conditions on `block_range` are expressed redundantly
                // to make more indexes useable
                sql_query(format!(
                    "/* controller=prune,phase=final,start_vid={next_vid},batch_size={batch_size} */ \
                     insert into {dst}({column_list}) \
                     select {column_list} from {src} \
                      where lower(block_range) <= $2 \
                        and coalesce(upper(block_range), 2147483647) > $1 \
                        and coalesce(upper(block_range), 2147483647) <= $2 \
                        and block_range && int4range($1, $2, '[]') \
                        and vid >= $3 and vid < $3 + $4 \
                      order by vid",
                    src = self.src.qualified_name,
                    dst = self.dst.qualified_name,
                    batch_size = batch_size.size,
                ))
                .bind::<Integer, _>(earliest_block)
                .bind::<Integer, _>(final_block)
                .bind::<BigInt, _>(next_vid)
                .bind::<BigInt, _>(&batch_size)
                .execute(conn)
            })?;
            cancel.check_cancel()?;

            next_vid += batch_size.size;

            batch_size.adapt(start.elapsed());

            reporter.prune_batch(
                self.src.name.as_str(),
                rows,
                PrunePhase::CopyFinal,
                next_vid > max_vid,
            );
        }
        Ok(())
    }

    /// Copy all entity versions visible after `final_block` in batches,
    /// where each batch is a separate transaction. This assumes that all
    /// other write activity to the source table is blocked while we copy
    fn copy_nonfinal_entities(
        &self,
        conn: &PgConnection,
        reporter: &mut dyn PruneReporter,
        final_block: BlockNumber,
    ) -> Result<(), StoreError> {
        let column_list = self.column_list();

        // Determine the last vid that we need to copy
        let (min_vid, max_vid) = self
            .src
            .vid_range(conn, final_block + 1, BLOCK_NUMBER_MAX)?;

        let mut batch_size = AdaptiveBatchSize::new(&self.src);
        // The first vid we still need to copy
        let mut next_vid = min_vid;
        while next_vid <= max_vid {
            let start = Instant::now();
            let rows = conn.transaction(|| {
                // Page through all the rows in `src` in batches of
                // `batch_size` that are visible to queries at block heights
                // starting right after `final_block`.
                // The conditions on `block_range` are expressed redundantly
                // to make more indexes useable
                sql_query(format!(
                    "/* controller=prune,phase=nonfinal,start_vid={next_vid},batch_size={batch_size} */ \
                     insert into {dst}({column_list}) \
                     select {column_list} from {src} \
                      where coalesce(upper(block_range), 2147483647) > $1 \
                        and block_range && int4range($1, null) \
                        and vid >= $2 and vid < $2 + $3 \
                      order by vid",
                    dst = self.dst.qualified_name,
                    src = self.src.qualified_name,
                    batch_size = batch_size.size
                ))
                .bind::<Integer, _>(final_block)
                .bind::<BigInt, _>(next_vid)
                .bind::<BigInt, _>(&batch_size)
                .execute(conn)
                .map_err(StoreError::from)
            })?;

            next_vid += batch_size.size;

            batch_size.adapt(start.elapsed());

            reporter.prune_batch(
                self.src.name.as_str(),
                rows,
                PrunePhase::CopyNonfinal,
                next_vid > max_vid,
            );
        }
        Ok(())
    }

    /// Replace the `src` table with the `dst` table
    fn switch(self, logger: &Logger, conn: &PgConnection) -> Result<(), StoreError> {
        let src_qname = &self.src.qualified_name;
        let dst_qname = &self.dst.qualified_name;
        let src_nsp = &self.src_nsp;
        let dst_nsp = &self.dst_nsp;

        let vid_seq = format!("{}_{VID_COLUMN}_seq", self.src.name);

        let mut query = String::new();

        // What we are about to do would get blocked by autovacuum on our
        // tables, so just kill the autovacuum
        if let Err(e) = catalog::cancel_vacuum(conn, src_nsp) {
            warn!(logger, "Failed to cancel vacuum during pruning; trying to carry on regardless";
                  "src" => src_nsp.as_str(), "error" => e.to_string());
        }

        // Make sure the vid sequence
        // continues from where it was
        writeln!(
            query,
            "select setval('{dst_nsp}.{vid_seq}', nextval('{src_nsp}.{vid_seq}'));"
        )?;

        writeln!(query, "drop table {src_qname};")?;
        writeln!(query, "alter table {dst_qname} set schema {src_nsp}")?;
        conn.transaction(|| conn.batch_execute(&query))?;

        Ok(())
    }

    fn column_list(&self) -> String {
        self.src
            .column_names()
            .map(|name| format!("\"{name}\""))
            .join(", ")
    }
}

impl Layout {
    /// Analyze the `tables` and return `VersionStats` for all tables in
    /// this `Layout`
    fn analyze_tables(
        &self,
        conn: &PgConnection,
        reporter: &mut dyn PruneReporter,
        mut tables: Vec<&Arc<Table>>,
        cancel: &CancelHandle,
    ) -> Result<Vec<VersionStats>, CancelableError<StoreError>> {
        reporter.start_analyze();
        tables.sort_by_key(|table| table.name.as_str());
        for table in &tables {
            reporter.start_analyze_table(table.name.as_str());
            table.analyze(conn)?;
            reporter.finish_analyze_table(table.name.as_str());
            cancel.check_cancel()?;
        }
        let stats = catalog::stats(conn, &self.site)?;

        let analyzed: Vec<_> = tables.iter().map(|table| table.name.as_str()).collect();
        reporter.finish_analyze(&stats, &analyzed);

        Ok(stats)
    }

    /// Return statistics for the tables in this `Layout`. If `analyze_all`
    /// is `true`, analyze all tables before getting statistics. If it is
    /// `false`, only analyze tables that Postgres' autovacuum daemon would
    /// consider needing analysis.
    fn version_stats(
        &self,
        conn: &PgConnection,
        reporter: &mut dyn PruneReporter,
        analyze_all: bool,
        cancel: &CancelHandle,
    ) -> Result<Vec<VersionStats>, CancelableError<StoreError>> {
        let needs_analyze = if analyze_all {
            vec![]
        } else {
            catalog::needs_autoanalyze(conn, &self.site.namespace)?
        };
        let tables: Vec<_> = self
            .tables
            .values()
            .filter(|table| analyze_all || needs_analyze.contains(&table.name))
            .collect();

        self.analyze_tables(conn, reporter, tables, cancel)
    }

    /// Return all tables and the strategy to prune them withir stats whose ratio of distinct entities
    /// to versions is less than `prune_ratio`
    fn prunable_tables(
        &self,
        stats: &[VersionStats],
        req: &PruneRequest,
    ) -> Vec<(&Arc<Table>, PruningStrategy)> {
        let mut prunable_tables = self
            .tables
            .values()
            .filter(|table| !table.immutable)
            .filter_map(|table| {
                stats
                    .iter()
                    .find(|stats| stats.tablename == table.name.as_str())
                    .map(|stats| (table, stats))
            })
            .filter_map(|(table, stats)| req.strategy(stats).map(|strat| (table, strat)))
            .collect::<Vec<_>>();
        prunable_tables.sort_by(|(a, _), (b, _)| a.name.as_str().cmp(b.name.as_str()));
        prunable_tables
    }

    /// Remove all data from the underlying deployment that is not needed to
    /// respond to queries before block `earliest_block`. The `req` is used
    /// to determine which strategy should be used for pruning, rebuild or
    /// delete.
    ///
    /// Blocks before `req.final_block` are considered final and it is
    /// assumed that they will not be modified in any way while pruning is
    /// running.
    ///
    /// The rebuild strategy implemented here works well for situations in
    /// which pruning will remove a large amount of data from the subgraph
    /// (say, at least 50%)
    ///
    /// The strategy for rebuilding is to copy all data that is needed to
    /// respond to queries at block heights at or after `earliest_block` to
    /// a new table and then to replace the existing tables with these new
    /// tables atomically in a transaction. Rebuilding happens in two stages
    /// that are performed for each table in turn: we first copy data for
    /// final blocks without blocking writes, and then copy data for
    /// nonfinal blocks. The latter blocks writes by taking an advisory lock
    /// on the deployment (via `deployment::lock`) The process for switching
    /// to the new tables needs to take the naming of various database
    /// objects that Postgres creates automatically into account so that
    /// they all have the same names as the original objects to ensure that
    /// pruning can be done again without risking name clashes.
    ///
    /// The reason this strategy works well when a lot (or even the
    /// majority) of the data needs to be removed is that in the more
    /// straightforward strategy of simply deleting unneeded data, accessing
    /// the remaining data becomes very inefficient since it is scattered
    /// over a large number of pages, often with just one row per page. We
    /// would therefore need to do a full vacuum of the tables after
    /// deleting which effectively copies the remaining data into new
    /// tables. But a full vacuum takes an `access exclusive` lock which
    /// prevents both reads and writes to the table, which means it would
    /// also block queries to the deployment, often for extended periods of
    /// time. The rebuild strategy never blocks reads, it only ever blocks
    /// writes.
    pub fn prune(
        &self,
        logger: &Logger,
        reporter: &mut dyn PruneReporter,
        conn: &PgConnection,
        req: &PruneRequest,
        cancel: &CancelHandle,
    ) -> Result<(), CancelableError<StoreError>> {
        reporter.start(req);

        let stats = self.version_stats(conn, reporter, true, cancel)?;

        let prunable_tables: Vec<_> = self.prunable_tables(&stats, req).into_iter().collect();

        // create a shadow namespace where we will put the copies of our
        // tables, but only create it in the database if we really need it
        let dst_nsp = Namespace::prune(self.site.id);
        let mut recreate_dst_nsp = true;

        // Go table by table; note that the subgraph writer can write in
        // between the execution of the `with_lock` block below, and might
        // therefore work with tables where some are pruned and some are not
        // pruned yet. That does not affect correctness since we make no
        // assumption about where the subgraph head is. If the subgraph
        // advances during this loop, we might have an unnecessarily
        // pessimistic but still safe value for `final_block`. We do assume
        // that `final_block` is far enough from the subgraph head that it
        // stays final even if a revert happens during this loop, but that
        // is the definition of 'final'
        for (table, strat) in &prunable_tables {
            reporter.start_table(table.name.as_str());
            match strat {
                PruningStrategy::Rebuild => {
                    if recreate_dst_nsp {
                        catalog::recreate_schema(conn, dst_nsp.as_str())?;
                        recreate_dst_nsp = false;
                    }
                    let pair = TablePair::create(
                        conn,
                        table.cheap_clone(),
                        self.site.namespace.clone(),
                        dst_nsp.clone(),
                        &self.catalog,
                    )?;
                    // Copy final entities. This can happen in parallel to indexing as
                    // that part of the table will not change
                    pair.copy_final_entities(
                        conn,
                        reporter,
                        req.earliest_block,
                        req.final_block,
                        cancel,
                    )?;
                    // Copy nonfinal entities, and replace the original `src` table with
                    // the smaller `dst` table
                    // see also: deployment-lock-for-update
                    reporter.start_switch();
                    deployment::with_lock(conn, &self.site, || -> Result<_, StoreError> {
                        pair.copy_nonfinal_entities(conn, reporter, req.final_block)?;
                        cancel.check_cancel().map_err(CancelableError::from)?;

                        conn.transaction(|| pair.switch(logger, conn))?;
                        cancel.check_cancel().map_err(CancelableError::from)?;

                        Ok(())
                    })?;
                    reporter.finish_switch();
                }
                PruningStrategy::Delete => {
                    // Delete all entity versions whose range was closed
                    // before `req.earliest_block`
                    let (min_vid, max_vid) = table.vid_range(conn, 0, req.earliest_block)?;
                    let mut batch_size = AdaptiveBatchSize::new(&table);
                    let mut next_vid = min_vid;
                    while next_vid <= max_vid {
                        let start = Instant::now();
                        let rows = sql_query(format!(
                            "/* controller=prune,phase=delete,start_vid={next_vid},batch_size={batch_size} */ \
                             delete from {qname} \
                                          where coalesce(upper(block_range), 2147483647) <= $1 \
                                            and vid >= $2 and vid < $2 + $3",
                            qname = table.qualified_name,
                            batch_size = batch_size.size
                        ))
                        .bind::<Integer, _>(req.earliest_block)
                        .bind::<BigInt, _>(next_vid)
                        .bind::<BigInt, _>(&batch_size)
                        .execute(conn)?;

                        next_vid += batch_size.size;

                        batch_size.adapt(start.elapsed());

                        reporter.prune_batch(
                            table.name.as_str(),
                            rows as usize,
                            PrunePhase::Delete,
                            next_vid > max_vid,
                        );
                    }
                }
            }
            reporter.finish_table(table.name.as_str());
        }
        // Get rid of the temporary prune schema if we actually created it
        if !recreate_dst_nsp {
            catalog::drop_schema(conn, dst_nsp.as_str())?;
        }

        for (table, _) in &prunable_tables {
            catalog::set_last_pruned_block(conn, &self.site, &table.name, req.earliest_block)?;
        }

        // Analyze the new tables
        let tables = prunable_tables.iter().map(|(table, _)| *table).collect();
        self.analyze_tables(conn, reporter, tables, cancel)?;

        reporter.finish();

        Ok(())
    }
}
