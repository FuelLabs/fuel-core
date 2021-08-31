use core::ops::Deref;
use diesel::{
    prelude::PgConnection,
    RunQueryDsl,
    sql_query,
    sql_types::Binary,
};
use r2d2_diesel::ConnectionManager;
use std::collections::HashMap;
use wasmer::Instance;

use fuel_indexer::types as ft;
use crate::runtime::ffi;
use crate::runtime::IndexerResult;
use crate::runtime::schema::{ColumnInfo, EntityData, RowCount};


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


pub struct SchemaManager {
    pool: DbPool
}


impl SchemaManager {
    pub fn new(db_conn: String) -> IndexerResult<SchemaManager> {
        let manager = ConnectionManager::<PgConnection>::new(db_conn);
        let pool = DbPool(r2d2::Pool::builder().build(manager)?);

        Ok(SchemaManager {
            pool,
        })
    }

    pub fn schema_exists(&self, name: &str) -> IndexerResult<bool> {
        let connection  = self.pool.get()?;

        // NOTE: do we want versioning and schema upgrades?
        let count: Vec<RowCount> = sql_query(format!(
            "select count(*)
            from graph_registry.type_ids
            where schema_name = '{}'", name)
        ).get_results(&*connection)?;

        Ok(count[0].count > 0)
    }

    pub fn new_schema(&self, name: &str, schema: &str) -> IndexerResult<()> {
        let connection  = self.pool.get()?;

        if !self.schema_exists(name)? {
            // TODO: need a diesel schema for metadata. For now,
            //       just placing it all in a file in the manifest.
            for query in schema.split(';') {
                sql_query(query).execute(&*connection)?;
            }
        }
        Ok(())
    }
}


#[derive(Clone, Debug)]
pub struct Database {
    pub pool: DbPool,
    pub namespace: String,
    pub schema: HashMap<String, Vec<String>>,
    pub tables: HashMap<u64, String>,
}


impl Database {
    pub fn new(db_conn: String) -> IndexerResult<Database> {
        let manager = ConnectionManager::<PgConnection>::new(db_conn);
        let pool = DbPool(r2d2::Pool::builder().build(manager)?);

        Ok(Database {
            pool,
            namespace: Default::default(),
            schema: Default::default(),
            tables: Default::default(),
        })
    }

    fn upsert_query(&self, table: &String, inserts: Vec<String>) -> String {
        format!("INSERT INTO {}.{}
                ({})
             VALUES
                ({}, $1)
             ON CONFLICT DO NOTHING",
            self.namespace,
            table,
            self.schema[table].join(", "),
            inserts.join(", "))
    }

    fn get_query(&self, table: &str, object_id: u64) -> String {
        format!(
            "SELECT object from {}.{} where id = {}",
            self.namespace, table, object_id
        )
    }

    pub fn put_object(&mut self, type_id: u64, columns: Vec<ft::FtColumn>, bytes: Vec<u8>) {
        let connection = self.pool.get().expect("connection pool failed");

        let table = &self.tables[&type_id];
        // TODO: think through this... is there a betteare diesel way??
        let inserts: Vec<_> = columns.iter().map(|col| {
            col.query_fragment()
        }).collect();

        let query_text = self.upsert_query(table, inserts);

        let query = sql_query(&query_text)
            .bind::<Binary, _>(bytes);

        query
            .execute(&*connection)
            .expect("Query failed");
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

    pub fn load_schema(&mut self, instance: &Instance) -> IndexerResult<()>{
        let connection = self.pool.get()?;
        self.namespace = ffi::get_namespace(instance)?;
        // TODO: create diesel schema for this....
        let schema_query = format!(
            "select
                type_id,table_name,column_position,column_name,column_type
            from graph_registry.type_ids ti
            join graph_registry.columns co
            on ti.id = co.type_id
            where schema_name = '{}'
            order by type_id,column_position", self.namespace);

        let results: Vec<ColumnInfo> = sql_query(&schema_query)
            .get_results(&*connection)?;

        for column in results {
            let table = &column.table_name;

            if !self.tables.contains_key(&(column.type_id as u64)) {
                self.tables.insert(column.type_id as u64, table.to_string());
            }

            let columns = self.schema
                .entry(table.to_string())
                .or_insert(vec![]);

            columns.push(column.column_name);
        }

        Ok(())
    }
}
