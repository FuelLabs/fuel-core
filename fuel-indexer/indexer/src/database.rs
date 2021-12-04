use core::ops::Deref;
use diesel::{
    connection::Connection, prelude::PgConnection, sql_query, sql_types::Binary, RunQueryDsl,
};
use r2d2_diesel::ConnectionManager;
use std::collections::HashMap;
use wasmer::Instance;

use crate::ffi;
use crate::IndexerResult;
use fuel_indexer_schema::{
    db::models::{ColumnInfo, EntityData, TypeIds},
    db::tables::SchemaBuilder,
    schema_version, FtColumn,
};

type PgConnectionPool = r2d2::Pool<ConnectionManager<PgConnection>>;

//#[cfg(test)]
//mod test_helpers {
//    use diesel::{connection::Connection, prelude::PgConnection};
//
//    #[derive(Debug)]
//    pub struct TestTransaction;
//
//    impl r2d2::CustomizeConnection<PgConnection, r2d2_diesel::Error> for TestTransaction {
//        fn on_acquire(&self, conn: &mut PgConnection) -> std::result::Result<(), r2d2_diesel::Error> {
//            conn.begin_test_transaction().unwrap();
//            Ok(())
//        }
//    }
//}

#[derive(Clone)]
pub struct DbPool(PgConnectionPool);

impl DbPool {
    //    #[cfg(not(test))]
    pub fn new(db_conn: impl Into<String>) -> IndexerResult<DbPool> {
        let manager = ConnectionManager::<PgConnection>::new(db_conn);
        Ok(DbPool(r2d2::Pool::builder().build(manager)?))
    }

    //#[cfg(test)]
    //pub fn new(db_conn: impl Into<String>) -> IndexerResult<DbPool> {
    //    let manager = ConnectionManager::<PgConnection>::new(db_conn);
    //    let pool = r2d2::Pool::builder()
    //        .connection_customizer(Box::new(test_helpers::TestTransaction))
    //        .max_size(1)
    //        .build(manager)?;
    //    Ok(DbPool(pool))
    //}
}

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
        let pool = DbPool::new(db_conn)?;

        Ok(SchemaManager { pool })
    }

    pub fn new_schema(&self, name: &str, schema: &str) -> IndexerResult<()> {
        let connection = self.pool.get()?;

        // TODO: Not doing much with version, but might be useful if we
        //       do graph schema upgrades
        let version = schema_version(schema);

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
        let pool = DbPool::new(db_conn)?;

        Ok(Database {
            pool,
            namespace: Default::default(),
            version: Default::default(),
            schema: Default::default(),
            tables: Default::default(),
        })
    }

    fn upsert_query(&self, table: &str, inserts: Vec<String>) -> String {
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

            self.tables
                .entry(column.type_id as u64)
                .or_insert_with(|| table.to_string());

            let columns = self
                .schema
                .entry(table.to_string())
                .or_insert_with(Vec::new);

            columns.push(column.column_name);
        }

        Ok(())
    }
}

#[cfg(feature = "postgres")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexEnv;
    use fuel_tx::Address;
    use wasmer::{imports, Instance, Module, Store, WasmerEnv};
    use wasmer_compiler_llvm::LLVM;
    use wasmer_engine_universal::Universal;

    const DATABASE_URL: &'static str = "postgres://postgres:my-secret@127.0.0.1:5432";
    const GRAPHQL_SCHEMA: &'static str = include_str!("test_data/schema.graphql");
    const WASM_BYTES: &'static [u8] = include_bytes!("test_data/simple_wasm.wasm");
    const THING1_TYPE: u64 = 0x89F35D3CD458C71E;
    const TEST_COLUMNS: [(&'static str, i32, &'static str); 7] = [
        ("thing1", 0, "id"),
        ("thing1", 1, "account"),
        ("thing1", 2, "object"),
        ("thing2", 0, "id"),
        ("thing2", 1, "account"),
        ("thing2", 2, "hash"),
        ("thing2", 3, "object"),
    ];

    fn wasm_instance() -> IndexerResult<Instance> {
        let compiler = LLVM::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = Module::new(&store, WASM_BYTES)?;

        let mut import_object = imports! {};

        let mut env = IndexEnv::new(DATABASE_URL.to_string())?;
        let exports = ffi::get_exports(&env, &store);
        import_object.register("env", exports);

        let instance = Instance::new(&module, &import_object)?;
        env.init_with_instance(&instance)?;
        Ok(instance)
    }

    #[test]
    fn test_schema_manager() {
        let manager = SchemaManager::new(DATABASE_URL).expect("Could not create SchemaManager");

        let result = manager.new_schema("test_namespace", GRAPHQL_SCHEMA);
        assert!(result.is_ok());

        let result = manager.new_schema("test_namespace", GRAPHQL_SCHEMA);
        assert!(result.is_ok());

        let pool = DbPool::new(DATABASE_URL).expect("Connection pool error");

        let version = schema_version(GRAPHQL_SCHEMA);
        let conn = pool.get().unwrap();
        let results = ColumnInfo::get_schema(&conn, "test_namespace", &version)
            .expect("Metadata query failed");

        for (index, result) in results.into_iter().enumerate() {
            assert_eq!(result.table_name, TEST_COLUMNS[index].0);
            assert_eq!(result.column_position, TEST_COLUMNS[index].1);
            assert_eq!(result.column_name, TEST_COLUMNS[index].2);
        }

        let instance = wasm_instance().expect("Error creating WASM module");

        let mut db = Database::new(DATABASE_URL).expect("Failed to create database object.");

        db.load_schema(&instance).expect("Could not load db schema");

        assert_eq!(db.namespace, "test_namespace");
        assert_eq!(db.version, version);

        for column in TEST_COLUMNS.iter() {
            assert!(db.schema.contains_key(column.0));
        }

        let object_id = 4;
        let columns = vec![
            FtColumn::ID(object_id),
            FtColumn::Address(Address::from([0x04; 32])),
        ];
        let bytes = vec![0u8, 1u8, 2u8, 3u8];
        db.put_object(THING1_TYPE, columns, bytes.clone());

        let obj = db.get_object(THING1_TYPE, object_id);
        assert!(obj.is_some());
        let obj = obj.unwrap();

        assert_eq!(obj, bytes);

        assert_eq!(db.get_object(THING1_TYPE, 90), None);
    }
}
