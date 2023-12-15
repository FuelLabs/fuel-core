use diesel::sql_types::{Bool, Integer};
use diesel::{connection::SimpleConnection, prelude::RunQueryDsl, select};
use diesel::{insert_into, OptionalExtension};
use diesel::{pg::PgConnection, sql_query};
use diesel::{
    sql_types::{Array, Double, Nullable, Text},
    ExpressionMethods, QueryDsl,
};
use graph::components::store::VersionStats;
use graph::prelude::BlockNumber;
use graph::schema::EntityType;
use itertools::Itertools;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::iter::FromIterator;
use std::sync::Arc;
use std::time::Duration;

use graph::prelude::anyhow::anyhow;
use graph::{
    data::subgraph::schema::POI_TABLE,
    prelude::{lazy_static, StoreError},
};

use crate::connection_pool::ForeignServer;
use crate::{
    primary::{Namespace, Site, NAMESPACE_PUBLIC},
    relational::SqlName,
};

// This is a view not a table. We only read from it
table! {
    information_schema.foreign_tables(foreign_table_schema, foreign_table_name) {
        foreign_table_catalog -> Text,
        foreign_table_schema -> Text,
        foreign_table_name -> Text,
        foreign_server_catalog -> Text,
        foreign_server_name -> Text,
    }
}

// Readonly;  not all columns are mapped
table! {
    pg_namespace(oid) {
        oid -> Oid,
        #[sql_name = "nspname"]
        name -> Text,
    }
}
// Readonly; only mapping the columns we want
table! {
    pg_database(datname) {
        datname -> Text,
        datcollate -> Text,
        datctype -> Text,
    }
}

// Readonly; not all columns are mapped
table! {
    pg_class(oid) {
        oid -> Oid,
        #[sql_name = "relname"]
        name -> Text,
        #[sql_name = "relnamespace"]
        namespace -> Oid,
        #[sql_name = "relpages"]
        pages -> Integer,
        #[sql_name = "reltuples"]
        tuples -> Integer,
        #[sql_name = "relkind"]
        kind -> Char,
        #[sql_name = "relnatts"]
        natts -> Smallint,
    }
}

// Readonly; not all columns are mapped
table! {
    pg_attribute(oid) {
        #[sql_name = "attrelid"]
        oid -> Oid,
        #[sql_name = "attrelid"]
        relid -> Oid,
        #[sql_name = "attname"]
        name -> Text,
        #[sql_name = "attnum"]
        num -> Smallint,
        #[sql_name = "attstattarget"]
        stats_target -> Integer,
    }
}

joinable!(pg_class -> pg_namespace(namespace));
joinable!(pg_attribute -> pg_class(relid));
allow_tables_to_appear_in_same_query!(pg_class, pg_namespace, pg_attribute);

table! {
    subgraphs.table_stats {
        id -> Integer,
        deployment -> Integer,
        table_name -> Text,
        is_account_like -> Nullable<Bool>,
        last_pruned_block -> Nullable<Integer>,
    }
}

table! {
    __diesel_schema_migrations(version) {
        version -> Text,
        run_on -> Timestamp,
    }
}

lazy_static! {
    /// The name of the table in which Diesel records migrations
    static ref MIGRATIONS_TABLE: SqlName =
        SqlName::verbatim("__diesel_schema_migrations".to_string());
}

pub struct Locale {
    collate: String,
    ctype: String,
    encoding: String,
}

impl Locale {
    /// Load locale information for current database
    pub fn load(conn: &PgConnection) -> Result<Locale, StoreError> {
        use diesel::dsl::sql;
        use pg_database as db;

        let (collate, ctype, encoding) = db::table
            .filter(db::datname.eq(sql("current_database()")))
            .select((
                db::datcollate,
                db::datctype,
                sql::<Text>("pg_encoding_to_char(encoding)::text"),
            ))
            .get_result::<(String, String, String)>(conn)?;
        Ok(Locale {
            collate,
            ctype,
            encoding,
        })
    }

    pub fn suitable(&self) -> Result<(), String> {
        if self.collate != "C" {
            return Err(format!(
                "database collation is `{}` but must be `C`",
                self.collate
            ));
        }
        if self.ctype != "C" {
            return Err(format!(
                "database ctype is `{}` but must be `C`",
                self.ctype
            ));
        }
        if self.encoding != "UTF8" {
            return Err(format!(
                "database encoding is `{}` but must be `UTF8`",
                self.encoding
            ));
        }
        Ok(())
    }
}

/// Information about what tables and columns we have in the database
#[derive(Debug, Clone)]
pub struct Catalog {
    pub site: Arc<Site>,
    text_columns: HashMap<String, HashSet<String>>,

    pub use_poi: bool,
    /// Whether `bytea` columns are indexed with just a prefix (`true`) or
    /// in their entirety. This influences both DDL generation and how
    /// queries are generated
    pub use_bytea_prefix: bool,

    /// Set of tables which have an explicit causality region column.
    pub(crate) entities_with_causality_region: BTreeSet<EntityType>,

    /// Whether the database supports `int4_minmax_multi_ops` etc.
    /// See the [Postgres docs](https://www.postgresql.org/docs/15/brin-builtin-opclasses.html)
    has_minmax_multi_ops: bool,
}

impl Catalog {
    /// Load the catalog for an existing subgraph
    pub fn load(
        conn: &PgConnection,
        site: Arc<Site>,
        use_bytea_prefix: bool,
        entities_with_causality_region: Vec<EntityType>,
    ) -> Result<Self, StoreError> {
        let text_columns = get_text_columns(conn, &site.namespace)?;
        let use_poi = supports_proof_of_indexing(conn, &site.namespace)?;
        let has_minmax_multi_ops = has_minmax_multi_ops(conn)?;

        Ok(Catalog {
            site,
            text_columns,
            use_poi,
            use_bytea_prefix,
            entities_with_causality_region: entities_with_causality_region.into_iter().collect(),
            has_minmax_multi_ops,
        })
    }

    /// Return a new catalog suitable for creating a new subgraph
    pub fn for_creation(
        conn: &PgConnection,
        site: Arc<Site>,
        entities_with_causality_region: BTreeSet<EntityType>,
    ) -> Result<Self, StoreError> {
        let has_minmax_multi_ops = has_minmax_multi_ops(conn)?;

        Ok(Catalog {
            site,
            text_columns: HashMap::default(),
            // DDL generation creates a POI table
            use_poi: true,
            // DDL generation creates indexes for prefixes of bytes columns
            // see: attr-bytea-prefix
            use_bytea_prefix: true,
            entities_with_causality_region,
            has_minmax_multi_ops,
        })
    }

    /// Make a catalog as if the given `schema` did not exist in the database
    /// yet. This function should only be used in situations where a database
    /// connection is definitely not available, such as in unit tests
    pub fn for_tests(
        site: Arc<Site>,
        entities_with_causality_region: BTreeSet<EntityType>,
    ) -> Result<Self, StoreError> {
        Ok(Catalog {
            site,
            text_columns: HashMap::default(),
            use_poi: false,
            use_bytea_prefix: true,
            entities_with_causality_region,
            has_minmax_multi_ops: false,
        })
    }

    /// Return `true` if `table` exists and contains the given `column` and
    /// if that column is of data type `text`
    pub fn is_existing_text_column(&self, table: &SqlName, column: &SqlName) -> bool {
        self.text_columns
            .get(table.as_str())
            .map(|cols| cols.contains(column.as_str()))
            .unwrap_or(false)
    }

    /// The operator classes to use for BRIN indexes. The first entry if the
    /// operator class for `int4`, the second is for `int8`
    pub fn minmax_ops(&self) -> (&str, &str) {
        const MINMAX_OPS: (&str, &str) = ("int4_minmax_ops", "int8_minmax_ops");
        const MINMAX_MULTI_OPS: (&str, &str) = ("int4_minmax_multi_ops", "int8_minmax_multi_ops");

        if self.has_minmax_multi_ops {
            MINMAX_MULTI_OPS
        } else {
            MINMAX_OPS
        }
    }
}

fn get_text_columns(
    conn: &PgConnection,
    namespace: &Namespace,
) -> Result<HashMap<String, HashSet<String>>, StoreError> {
    const QUERY: &str = "
        select table_name, column_name
          from information_schema.columns
         where table_schema = $1 and data_type = 'text'";

    #[derive(Debug, QueryableByName)]
    struct Column {
        #[sql_type = "Text"]
        pub table_name: String,
        #[sql_type = "Text"]
        pub column_name: String,
    }

    let map: HashMap<String, HashSet<String>> = diesel::sql_query(QUERY)
        .bind::<Text, _>(namespace.as_str())
        .load::<Column>(conn)?
        .into_iter()
        .fold(HashMap::new(), |mut map, col| {
            map.entry(col.table_name)
                .or_default()
                .insert(col.column_name);
            map
        });
    Ok(map)
}

pub fn table_exists(
    conn: &PgConnection,
    namespace: &str,
    table: &SqlName,
) -> Result<bool, StoreError> {
    #[derive(Debug, QueryableByName)]
    struct Table {
        #[sql_type = "Text"]
        #[allow(dead_code)]
        pub table_name: String,
    }
    let query =
        "SELECT table_name FROM information_schema.tables WHERE table_schema=$1 AND table_name=$2";
    let result: Vec<Table> = diesel::sql_query(query)
        .bind::<Text, _>(namespace)
        .bind::<Text, _>(table.as_str())
        .load(conn)?;
    Ok(!result.is_empty())
}

pub fn supports_proof_of_indexing(
    conn: &diesel::pg::PgConnection,
    namespace: &Namespace,
) -> Result<bool, StoreError> {
    lazy_static! {
        static ref POI_TABLE_NAME: SqlName = SqlName::verbatim(POI_TABLE.to_owned());
    }
    table_exists(conn, namespace.as_str(), &POI_TABLE_NAME)
}

pub fn current_servers(conn: &PgConnection) -> Result<Vec<String>, StoreError> {
    #[derive(QueryableByName)]
    struct Srv {
        #[sql_type = "Text"]
        srvname: String,
    }
    Ok(sql_query("select srvname from pg_foreign_server")
        .get_results::<Srv>(conn)?
        .into_iter()
        .map(|srv| srv.srvname)
        .collect())
}

/// Return the options for the foreign server `name` as a map of option
/// names to values
pub fn server_options(
    conn: &PgConnection,
    name: &str,
) -> Result<HashMap<String, Option<String>>, StoreError> {
    #[derive(QueryableByName)]
    struct Srv {
        #[sql_type = "Array<Text>"]
        srvoptions: Vec<String>,
    }
    let entries = sql_query("select srvoptions from pg_foreign_server where srvname = $1")
        .bind::<Text, _>(name)
        .get_result::<Srv>(conn)?
        .srvoptions
        .into_iter()
        .filter_map(|opt| {
            let mut parts = opt.splitn(2, '=');
            let key = parts.next();
            let value = parts.next().map(|value| value.to_string());

            key.map(|key| (key.to_string(), value))
        });
    Ok(HashMap::from_iter(entries))
}

pub fn has_namespace(conn: &PgConnection, namespace: &Namespace) -> Result<bool, StoreError> {
    use pg_namespace as nsp;

    Ok(select(diesel::dsl::exists(
        nsp::table.filter(nsp::name.eq(namespace.as_str())),
    ))
    .get_result::<bool>(conn)?)
}

/// Drop the schema for `src` if it is a foreign schema imported from
/// another database. If the schema does not exist, or is not a foreign
/// schema, do nothing. This crucially depends on the fact that we never mix
/// foreign and local tables in the same schema.
pub fn drop_foreign_schema(conn: &PgConnection, src: &Site) -> Result<(), StoreError> {
    use foreign_tables as ft;

    let is_foreign = select(diesel::dsl::exists(
        ft::table.filter(ft::foreign_table_schema.eq(src.namespace.as_str())),
    ))
    .get_result::<bool>(conn)?;

    if is_foreign {
        let query = format!("drop schema if exists {} cascade", src.namespace);
        conn.batch_execute(&query)?;
    }
    Ok(())
}

/// Drop the schema `nsp` and all its contents if it exists, and create it
/// again so that `nsp` is an empty schema
pub fn recreate_schema(conn: &PgConnection, nsp: &str) -> Result<(), StoreError> {
    let query = format!(
        "drop schema if exists {nsp} cascade;\
         create schema {nsp};",
        nsp = nsp
    );
    Ok(conn.batch_execute(&query)?)
}

/// Drop the schema `nsp` and all its contents if it exists
pub fn drop_schema(conn: &PgConnection, nsp: &str) -> Result<(), StoreError> {
    let query = format!("drop schema if exists {nsp} cascade;", nsp = nsp);
    Ok(conn.batch_execute(&query)?)
}

pub fn migration_count(conn: &PgConnection) -> Result<usize, StoreError> {
    use __diesel_schema_migrations as m;

    if !table_exists(conn, NAMESPACE_PUBLIC, &MIGRATIONS_TABLE)? {
        return Ok(0);
    }

    m::table
        .count()
        .get_result(conn)
        .map(|n: i64| n as usize)
        .map_err(StoreError::from)
}

pub fn account_like(conn: &PgConnection, site: &Site) -> Result<HashSet<String>, StoreError> {
    use table_stats as ts;
    let names = ts::table
        .filter(ts::deployment.eq(site.id))
        .select((ts::table_name, ts::is_account_like))
        .get_results::<(String, Option<bool>)>(conn)
        .optional()?
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, account_like)| {
            if account_like == Some(true) {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    Ok(names)
}

pub fn set_account_like(
    conn: &PgConnection,
    site: &Site,
    table_name: &SqlName,
    is_account_like: bool,
) -> Result<(), StoreError> {
    use table_stats as ts;
    insert_into(ts::table)
        .values((
            ts::deployment.eq(site.id),
            ts::table_name.eq(table_name.as_str()),
            ts::is_account_like.eq(is_account_like),
        ))
        .on_conflict((ts::deployment, ts::table_name))
        .do_update()
        .set(ts::is_account_like.eq(is_account_like))
        .execute(conn)?;
    Ok(())
}

pub fn copy_account_like(conn: &PgConnection, src: &Site, dst: &Site) -> Result<usize, StoreError> {
    let src_nsp = ForeignServer::metadata_schema_in(&src.shard, &dst.shard);
    let query = format!(
        "insert into subgraphs.table_stats(deployment, table_name, is_account_like, last_pruned_block)
         select $2 as deployment, ts.table_name, ts.is_account_like, ts.last_pruned_block
           from {src_nsp}.table_stats ts
          where ts.deployment = $1",
        src_nsp = src_nsp
    );
    Ok(sql_query(query)
        .bind::<Integer, _>(src.id)
        .bind::<Integer, _>(dst.id)
        .execute(conn)?)
}

pub fn set_last_pruned_block(
    conn: &PgConnection,
    site: &Site,
    table_name: &SqlName,
    last_pruned_block: BlockNumber,
) -> Result<(), StoreError> {
    use table_stats as ts;

    insert_into(ts::table)
        .values((
            ts::deployment.eq(site.id),
            ts::table_name.eq(table_name.as_str()),
            ts::last_pruned_block.eq(last_pruned_block),
        ))
        .on_conflict((ts::deployment, ts::table_name))
        .do_update()
        .set(ts::last_pruned_block.eq(last_pruned_block))
        .execute(conn)?;
    Ok(())
}

pub(crate) mod table_schema {
    use super::*;

    /// The name and data type for the column in a table. The data type is
    /// in a form that it can be used in a `create table` statement
    pub struct Column {
        pub column_name: String,
        pub data_type: String,
    }

    #[derive(QueryableByName)]
    struct ColumnInfo {
        #[sql_type = "Text"]
        column_name: String,
        #[sql_type = "Text"]
        data_type: String,
        #[sql_type = "Text"]
        udt_name: String,
        #[sql_type = "Text"]
        udt_schema: String,
        #[sql_type = "Nullable<Text>"]
        elem_type: Option<String>,
    }

    impl From<ColumnInfo> for Column {
        fn from(ci: ColumnInfo) -> Self {
            // See description of `data_type` in
            // https://www.postgresql.org/docs/current/infoschema-columns.html
            let data_type = match ci.data_type.as_str() {
                "ARRAY" => format!(
                    "{}[]",
                    ci.elem_type.expect("array columns have an elem_type")
                ),
                "USER-DEFINED" => format!("{}.{}", ci.udt_schema, ci.udt_name),
                _ => ci.data_type.clone(),
            };
            Self {
                column_name: ci.column_name,
                data_type,
            }
        }
    }

    pub fn columns(
        conn: &PgConnection,
        nsp: &str,
        table_name: &str,
    ) -> Result<Vec<Column>, StoreError> {
        const QUERY: &str = " \
    select c.column_name::text, c.data_type::text,
           c.udt_name::text, c.udt_schema::text, e.data_type::text as elem_type
      from information_schema.columns c
      left join information_schema.element_types e
           on ((c.table_catalog, c.table_schema, c.table_name, 'TABLE', c.dtd_identifier)
            = (e.object_catalog, e.object_schema, e.object_name, e.object_type, e.collection_type_identifier))
     where c.table_schema = $1
       and c.table_name = $2
     order by c.ordinal_position";

        Ok(sql_query(QUERY)
            .bind::<Text, _>(nsp)
            .bind::<Text, _>(table_name)
            .get_results::<ColumnInfo>(conn)?
            .into_iter()
            .map(|ci| ci.into())
            .collect())
    }
}

/// Return a SQL statement to create the foreign table
/// `{dst_nsp}.{table_name}` for the server `server` which has the same
/// schema as the (local) table `{src_nsp}.{table_name}`
pub fn create_foreign_table(
    conn: &PgConnection,
    src_nsp: &str,
    table_name: &str,
    dst_nsp: &str,
    server: &str,
) -> Result<String, StoreError> {
    fn build_query(
        columns: Vec<table_schema::Column>,
        src_nsp: &str,
        table_name: &str,
        dst_nsp: &str,
        server: &str,
    ) -> Result<String, std::fmt::Error> {
        let mut query = String::new();
        write!(
            query,
            "create foreign table \"{}\".\"{}\" (",
            dst_nsp, table_name
        )?;
        for (idx, column) in columns.into_iter().enumerate() {
            if idx > 0 {
                write!(query, ", ")?;
            }
            write!(query, "\"{}\" {}", column.column_name, column.data_type)?;
        }
        writeln!(
            query,
            ") server \"{}\" options(schema_name '{}');",
            server, src_nsp
        )?;
        Ok(query)
    }

    let columns = table_schema::columns(conn, src_nsp, table_name)?;
    let query = build_query(columns, src_nsp, table_name, dst_nsp, server).map_err(|_| {
        anyhow!(
            "failed to generate 'create foreign table' query for {}.{}",
            dst_nsp,
            table_name
        )
    })?;
    Ok(query)
}

/// Checks in the database if a given index is valid.
pub(crate) fn check_index_is_valid(
    conn: &PgConnection,
    schema_name: &str,
    index_name: &str,
) -> Result<bool, StoreError> {
    #[derive(Queryable, QueryableByName)]
    struct ManualIndexCheck {
        #[sql_type = "Bool"]
        is_valid: bool,
    }

    let query = "
        select
            i.indisvalid as is_valid
        from
            pg_class c
            join pg_index i on i.indexrelid = c.oid
            join pg_namespace n on c.relnamespace = n.oid
        where
            n.nspname = $1
            and c.relname = $2";
    let result = sql_query(query)
        .bind::<Text, _>(schema_name)
        .bind::<Text, _>(index_name)
        .get_result::<ManualIndexCheck>(conn)
        .optional()
        .map_err::<StoreError, _>(Into::into)?
        .map(|check| check.is_valid);
    Ok(matches!(result, Some(true)))
}

pub(crate) fn indexes_for_table(
    conn: &PgConnection,
    schema_name: &str,
    table_name: &str,
) -> Result<Vec<String>, StoreError> {
    #[derive(Queryable, QueryableByName)]
    struct IndexName {
        #[sql_type = "Text"]
        #[column_name = "indexdef"]
        def: String,
    }

    let query = "
        select
            indexdef
        from
            pg_indexes
        where
            schemaname = $1
            and tablename = $2
        order by indexname";
    let results = sql_query(query)
        .bind::<Text, _>(schema_name)
        .bind::<Text, _>(table_name)
        .load::<IndexName>(conn)
        .map_err::<StoreError, _>(Into::into)?;

    Ok(results.into_iter().map(|i| i.def).collect())
}
pub(crate) fn drop_index(
    conn: &PgConnection,
    schema_name: &str,
    index_name: &str,
) -> Result<(), StoreError> {
    let query = format!("drop index concurrently {schema_name}.{index_name}");
    sql_query(query)
        .bind::<Text, _>(schema_name)
        .bind::<Text, _>(index_name)
        .execute(conn)
        .map_err::<StoreError, _>(Into::into)?;
    Ok(())
}

pub fn stats(conn: &PgConnection, site: &Site) -> Result<Vec<VersionStats>, StoreError> {
    #[derive(Queryable, QueryableByName)]
    pub struct DbStats {
        #[sql_type = "Integer"]
        pub entities: i32,
        #[sql_type = "Integer"]
        pub versions: i32,
        #[sql_type = "Text"]
        pub tablename: String,
        /// The ratio `entities / versions`
        #[sql_type = "Double"]
        pub ratio: f64,
        #[sql_type = "Nullable<Integer>"]
        pub last_pruned_block: Option<i32>,
    }

    impl From<DbStats> for VersionStats {
        fn from(s: DbStats) -> Self {
            VersionStats {
                entities: s.entities,
                versions: s.versions,
                tablename: s.tablename,
                ratio: s.ratio,
                last_pruned_block: s.last_pruned_block,
            }
        }
    }

    // Get an estimate of number of rows (pg_class.reltuples) and number of
    // distinct entities (based on the planners idea of how many distinct
    // values there are in the `id` column) See the [Postgres
    // docs](https://www.postgresql.org/docs/current/view-pg-stats.html) for
    // the precise meaning of n_distinct
    let query = "select case when s.n_distinct < 0 then (- s.n_distinct * c.reltuples)::int4
                     else s.n_distinct::int4
                 end as entities,
                 c.reltuples::int4  as versions,
                 c.relname as tablename,
                case when c.reltuples = 0 then 0::float8
                     when s.n_distinct < 0 then (-s.n_distinct)::float8
                     else greatest(s.n_distinct, 1)::float8 / c.reltuples::float8
                 end as ratio,
                 ts.last_pruned_block
           from pg_namespace n, pg_class c, pg_stats s
                left outer join subgraphs.table_stats ts
                     on (ts.table_name = s.tablename
                     and ts.deployment = $1)
          where n.nspname = $2
            and c.relnamespace = n.oid
            and s.schemaname = n.nspname
            and s.attname = 'id'
            and c.relname = s.tablename
          order by c.relname"
        .to_string();

    let stats = sql_query(query)
        .bind::<Integer, _>(site.id)
        .bind::<Text, _>(site.namespace.as_str())
        .load::<DbStats>(conn)
        .map_err(StoreError::from)?;

    Ok(stats.into_iter().map(|s| s.into()).collect())
}

/// Return by how much the slowest replica connected to the database `conn`
/// is lagging. The returned value has millisecond precision. If the
/// database has no replicas, return `0`
pub(crate) fn replication_lag(conn: &PgConnection) -> Result<Duration, StoreError> {
    #[derive(Queryable, QueryableByName)]
    struct Lag {
        #[sql_type = "Nullable<Integer>"]
        ms: Option<i32>,
    }

    let lag = sql_query(
        "select (extract(epoch from max(greatest(write_lag, flush_lag, replay_lag)))*1000)::int as ms \
           from pg_stat_replication",
    )
    .get_result::<Lag>(conn)?;

    let lag = lag
        .ms
        .map(|ms| if ms <= 0 { 0 } else { ms as u64 })
        .unwrap_or(0);

    Ok(Duration::from_millis(lag))
}

pub(crate) fn cancel_vacuum(conn: &PgConnection, namespace: &Namespace) -> Result<(), StoreError> {
    sql_query(
        "select pg_cancel_backend(v.pid) \
           from pg_stat_progress_vacuum v, \
                pg_class c, \
                pg_namespace n \
          where v.relid = c.oid \
            and c.relnamespace = n.oid \
            and n.nspname = $1",
    )
    .bind::<Text, _>(namespace)
    .execute(conn)?;
    Ok(())
}

pub(crate) fn default_stats_target(conn: &PgConnection) -> Result<i32, StoreError> {
    #[derive(Queryable, QueryableByName)]
    struct Target {
        #[sql_type = "Integer"]
        setting: i32,
    }

    let target =
        sql_query("select setting::int from pg_settings where name = 'default_statistics_target'")
            .get_result::<Target>(conn)?;
    Ok(target.setting)
}

pub(crate) fn stats_targets(
    conn: &PgConnection,
    namespace: &Namespace,
) -> Result<BTreeMap<SqlName, BTreeMap<SqlName, i32>>, StoreError> {
    use pg_attribute as a;
    use pg_class as c;
    use pg_namespace as n;

    let targets = c::table
        .inner_join(n::table)
        .inner_join(a::table)
        .filter(c::kind.eq("r"))
        .filter(n::name.eq(namespace.as_str()))
        .filter(a::num.ge(1))
        .select((c::name, a::name, a::stats_target))
        .load::<(String, String, i32)>(conn)?
        .into_iter()
        .map(|(table, column, target)| (SqlName::from(table), SqlName::from(column), target));

    let map = targets.into_iter().fold(
        BTreeMap::<SqlName, BTreeMap<SqlName, i32>>::new(),
        |mut map, (table, column, target)| {
            map.entry(table).or_default().insert(column, target);
            map
        },
    );
    Ok(map)
}

pub(crate) fn set_stats_target(
    conn: &PgConnection,
    namespace: &Namespace,
    table: &SqlName,
    columns: &[&SqlName],
    target: i32,
) -> Result<(), StoreError> {
    let columns = columns
        .iter()
        .map(|column| format!("alter column {} set statistics {}", column.quoted(), target))
        .join(", ");
    let query = format!("alter table {}.{} {}", namespace, table.quoted(), columns);
    conn.batch_execute(&query)?;
    Ok(())
}

/// Return the names of all tables in the `namespace` that need to be
/// analyzed. Whether a table needs to be analyzed is determined with the
/// same logic that Postgres' [autovacuum
/// daemon](https://www.postgresql.org/docs/current/routine-vacuuming.html#AUTOVACUUM)
/// uses
pub(crate) fn needs_autoanalyze(
    conn: &PgConnection,
    namespace: &Namespace,
) -> Result<Vec<SqlName>, StoreError> {
    const QUERY: &str = "select relname \
                           from pg_stat_user_tables \
                          where (select setting::numeric from pg_settings where name = 'autovacuum_analyze_threshold') \
                              + (select setting::numeric from pg_settings where name = 'autovacuum_analyze_scale_factor')*(n_live_tup + n_dead_tup) < n_mod_since_analyze
                            and schemaname = $1";

    #[derive(Queryable, QueryableByName)]
    struct TableName {
        #[sql_type = "Text"]
        name: SqlName,
    }

    let tables = sql_query(QUERY)
        .bind::<Text, _>(namespace.as_str())
        .get_results::<TableName>(conn)
        .optional()?
        .map(|tables| tables.into_iter().map(|t| t.name).collect())
        .unwrap_or(vec![]);

    Ok(tables)
}

/// Check whether the database for `conn` supports the `minmax_multi_ops`
/// introduced in Postgres 14
fn has_minmax_multi_ops(conn: &PgConnection) -> Result<bool, StoreError> {
    const QUERY: &str = "select count(*) = 2 as has_ops \
                           from pg_opclass \
                          where opcname in('int8_minmax_multi_ops', 'int4_minmax_multi_ops')";

    #[derive(Queryable, QueryableByName)]
    struct Ops {
        #[sql_type = "Bool"]
        has_ops: bool,
    }

    Ok(sql_query(QUERY).get_result::<Ops>(conn)?.has_ops)
}
