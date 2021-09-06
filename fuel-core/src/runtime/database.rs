use core::ops::Deref;
use diesel::{connection::Connection, prelude::PgConnection, sql_query, sql_types::Binary, RunQueryDsl};
use r2d2_diesel::ConnectionManager;
use std::collections::HashMap;
use wasmer::Instance;

use crate::runtime::postgres::SchemaBuilder;
use crate::runtime::ffi;
use crate::runtime::IndexerResult;
use fuel_indexer_schema::{
    FtColumn,
    schema_version,
    models::{ColumnInfo, EntityData, TypeIds}
};

type PgConnectionPool = r2d2::Pool<ConnectionManager<PgConnection>>;

#[derive(Clone)]
pub struct DbPool(PgConnectionPool);

impl Deref for DbPool {
    type Target = PgConnectionPool;

    fn deref(&self) -> &PgConnectionPool {
        &self.0
    }
}

impl std::fmt::Debug for DbPool {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "DBPool(...)")
    }
}

/// Responsible for laying down graph schemas, processes schema upgrades.
pub struct SchemaManager {
    pool: DbPool,
}

impl SchemaManager {
    pub fn new(db_conn: impl Into<String>) -> IndexerResult<SchemaManager> {
        let manager = ConnectionManager::<PgConnection>::new(db_conn);
        let pool = DbPool(r2d2::Pool::builder().build(manager)?);

        Ok(SchemaManager { pool })
    }

    pub fn new_schema(&self, name: &str, schema: &str) -> IndexerResult<()> {
        let connection = self.pool.get()?;

        // TODO: Not doing much with version, but might be useful if we
        //       do graph schema upgrades
        let version = schema_version(&schema);

        if !TypeIds::schema_exists(&*connection, name, &version)? {
            let db_schema = SchemaBuilder::new(name, &version).build(schema);

            connection.transaction::<(), diesel::result::Error, _>(|| {
                for query in db_schema.statements.iter() {
                    sql_query(query).execute(&*connection)?;
                }

                db_schema.commit_metadata(&*connection)?;

                Ok(())
            })?;
        }
        Ok(())
    }
}

/// Database for an executor instance, with schema info.
#[derive(Clone, Debug)]
pub struct Database {
    pub pool: DbPool,
    pub namespace: String,
    pub version: String,
    pub schema: HashMap<String, Vec<String>>,
    pub tables: HashMap<u64, String>,
}

impl Database {
    pub fn new(db_conn: impl Into<String>) -> IndexerResult<Database> {
        let manager = ConnectionManager::<PgConnection>::new(db_conn);
        let pool = DbPool(r2d2::Pool::builder().build(manager)?);

        Ok(Database {
            pool,
            namespace: Default::default(),
            version: Default::default(),
            schema: Default::default(),
            tables: Default::default(),
        })
    }

    fn upsert_query(&self, table: &String, inserts: Vec<String>) -> String {
        format!(
            "INSERT INTO {}.{}
                ({})
             VALUES
                ({}, $1)
             ON CONFLICT DO NOTHING",
            self.namespace,
            table,
            self.schema[table].join(", "),
            inserts.join(", ")
        )
    }

    fn get_query(&self, table: &str, object_id: u64) -> String {
        format!(
            "SELECT object from {}.{} where id = {}",
            self.namespace, table, object_id
        )
    }

    pub fn put_object(&mut self, type_id: u64, columns: Vec<FtColumn>, bytes: Vec<u8>) {
        let connection = self.pool.get().expect("connection pool failed");

        let table = &self.tables[&type_id];
        let inserts: Vec<_> = columns.iter().map(|col| col.query_fragment()).collect();

        let query_text = self.upsert_query(table, inserts);

        let query = sql_query(&query_text).bind::<Binary, _>(bytes);

        query.execute(&*connection).expect("Query failed");
    }

    pub fn get_object(&self, type_id: u64, object_id: u64) -> Option<Vec<u8>> {
        let connection = self.pool.get().expect("connection pool failed");
        let table = &self.tables[&type_id];

        let query = self.get_query(table, object_id);
        let mut row: Vec<EntityData> = sql_query(&query)
            .get_results(&*connection)
            .expect("Query failed");

        row.pop().map(|e| e.object)
    }

    pub fn load_schema(&mut self, instance: &Instance) -> IndexerResult<()> {
        let connection = self.pool.get()?;
        self.namespace = ffi::get_namespace(instance)?;
        self.version = ffi::get_version(instance)?;

        let results = ColumnInfo::get_schema(&*connection, &self.namespace, &self.version)?;

        for column in results {
            let table = &column.table_name;

            if !self.tables.contains_key(&(column.type_id as u64)) {
                self.tables.insert(column.type_id as u64, table.to_string());
            }

            let columns = self.schema.entry(table.to_string()).or_insert(vec![]);

            columns.push(column.column_name);
        }

        Ok(())
    }
}
