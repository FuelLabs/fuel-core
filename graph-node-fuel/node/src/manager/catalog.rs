pub mod pg_catalog {
    diesel::table! {
        pg_catalog.pg_stat_database (datid) {
            datid -> Oid,
            datname -> Nullable<Text>,
            numbackends -> Nullable<Int4>,
            xact_commit -> Nullable<Int8>,
            xact_rollback -> Nullable<Int8>,
            blks_read -> Nullable<Int8>,
            blks_hit -> Nullable<Int8>,
            tup_returned -> Nullable<Int8>,
            tup_fetched -> Nullable<Int8>,
            tup_inserted -> Nullable<Int8>,
            tup_updated -> Nullable<Int8>,
            tup_deleted -> Nullable<Int8>,
            conflicts -> Nullable<Int8>,
            temp_files -> Nullable<Int8>,
            temp_bytes -> Nullable<Int8>,
            deadlocks -> Nullable<Int8>,
            blk_read_time -> Nullable<Float8>,
            blk_write_time -> Nullable<Float8>,
            stats_reset -> Nullable<Timestamptz>,
        }
    }

    diesel::table! {
        pg_catalog.pg_stat_user_indexes (relid) {
            relid -> Oid,
            indexrelid -> Nullable<Oid>,
            schemaname -> Nullable<Text>,
            relname -> Nullable<Text>,
            indexrelname -> Nullable<Text>,
            idx_scan -> Nullable<Int8>,
            idx_tup_read -> Nullable<Int8>,
            idx_tup_fetch -> Nullable<Int8>,
        }
    }

    diesel::table! {
        pg_catalog.pg_stat_user_tables (relid) {
            relid -> Oid,
            schemaname -> Nullable<Text>,
            relname -> Nullable<Text>,
            seq_scan -> Nullable<Int8>,
            seq_tup_read -> Nullable<Int8>,
            idx_scan -> Nullable<Int8>,
            idx_tup_fetch -> Nullable<Int8>,
            n_tup_ins -> Nullable<Int8>,
            n_tup_upd -> Nullable<Int8>,
            n_tup_del -> Nullable<Int8>,
            n_tup_hot_upd -> Nullable<Int8>,
            n_live_tup -> Nullable<Int8>,
            n_dead_tup -> Nullable<Int8>,
            n_mod_since_analyze -> Nullable<Int8>,
            last_vacuum -> Nullable<Timestamptz>,
            last_autovacuum -> Nullable<Timestamptz>,
            last_analyze -> Nullable<Timestamptz>,
            last_autoanalyze -> Nullable<Timestamptz>,
            vacuum_count -> Nullable<Int8>,
            autovacuum_count -> Nullable<Int8>,
            analyze_count -> Nullable<Int8>,
            autoanalyze_count -> Nullable<Int8>,
        }
    }

    diesel::allow_tables_to_appear_in_same_query!(
        pg_stat_database,
        pg_stat_user_indexes,
        pg_stat_user_tables,
    );
}
