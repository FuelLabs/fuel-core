use crate::db::schema::graph_registry as gr;
use crate::sql_types::Columntypename;
use crate::ColumnType;
use diesel::prelude::*;
use diesel::{result::QueryResult, sql_types::*};
use gr::{columns, graph_root, root_columns, type_ids};

#[derive(Insertable, Queryable, QueryableByName)]
#[table_name = "root_columns"]
#[allow(unused)]
pub struct RootColumns {
    pub id: i32,
    pub root_id: i64,
    pub column_name: String,
    pub graphql_type: String,
}

impl RootColumns {
    pub fn list_by_id(conn: &PgConnection, r_id: i64) -> QueryResult<Vec<RootColumns>> {
        use gr::root_columns::dsl::*;
        root_columns.filter(root_id.eq(r_id)).load(conn)
    }
}

#[derive(Insertable, Queryable, QueryableByName)]
#[table_name = "root_columns"]
pub struct NewRootColumns {
    pub root_id: i64,
    pub column_name: String,
    pub graphql_type: String,
}

impl NewRootColumns {
    pub fn insert(self, conn: &PgConnection) -> QueryResult<usize> {
        use gr::root_columns::dsl::*;
        diesel::insert_into(root_columns).values(self).execute(conn)
    }
}

#[derive(Insertable, Queryable, QueryableByName)]
#[table_name = "graph_root"]
pub struct NewGraphRoot {
    pub version: String,
    pub schema_name: String,
    pub query: String,
    pub schema: String,
}

impl NewGraphRoot {
    pub fn insert(self, conn: &PgConnection) -> QueryResult<usize> {
        use gr::graph_root::dsl::*;
        diesel::insert_into(graph_root).values(self).execute(conn)
    }
}

#[derive(Insertable, Queryable, QueryableByName)]
#[table_name = "graph_root"]
#[allow(unused)]
pub struct GraphRoot {
    pub id: i64,
    pub version: String,
    pub schema_name: String,
    pub query: String,
    pub schema: String,
}

impl GraphRoot {
    pub fn get_latest(conn: &PgConnection, name: &str) -> QueryResult<GraphRoot> {
        use gr::graph_root::dsl::*;
        graph_root
            .filter(schema_name.eq(name))
            .order_by(id.desc())
            .first(conn)
    }
}

#[derive(Insertable, Queryable, QueryableByName)]
#[table_name = "type_ids"]
#[allow(unused)]
pub struct TypeIds {
    pub id: i64,
    pub schema_version: String,
    pub schema_name: String,
    pub graphql_name: String,
    pub table_name: String,
}

impl TypeIds {
    pub fn list_by_name(
        conn: &PgConnection,
        name: &str,
        version: &str,
    ) -> QueryResult<Vec<TypeIds>> {
        use gr::type_ids::dsl::*;
        type_ids
            .filter(schema_name.eq(name).and(schema_version.eq(version)))
            .load(conn)
    }

    pub fn insert(&self, conn: &PgConnection) -> QueryResult<usize> {
        let result = diesel::insert_into(gr::type_ids::table)
            .values(self)
            .execute(conn)?;
        Ok(result)
    }

    pub fn schema_exists(conn: &PgConnection, name: &str, version: &str) -> QueryResult<bool> {
        use gr::type_ids::dsl::*;

        let result: i64 = type_ids
            .filter(schema_name.eq(name).and(schema_version.eq(version)))
            .count()
            .get_result(conn)?;

        Ok(result != 0)
    }
}

#[derive(Insertable, Queryable, QueryableByName)]
#[table_name = "columns"]
pub struct NewColumn {
    pub type_id: i64,
    pub column_position: i32,
    pub column_name: String,
    pub column_type: ColumnType,
    pub nullable: bool,
    pub graphql_type: String,
}

impl NewColumn {
    pub fn insert(&self, conn: &PgConnection) -> QueryResult<usize> {
        let result = diesel::insert_into(gr::columns::table)
            .values(self)
            .execute(conn)?;
        Ok(result)
    }

    pub fn sql_fragment(&self) -> String {
        if self.nullable {
            format!("{} {}", self.column_name, self.sql_type())
        } else {
            format!("{} {} not null", self.column_name, self.sql_type())
        }
    }

    fn sql_type(&self) -> &str {
        match self.column_type {
            ColumnType::ID => "bigint primary key",
            ColumnType::Address => "varchar(64)",
            ColumnType::Bytes4 => "varchar(8)",
            ColumnType::Bytes8 => "varchar(16)",
            ColumnType::Bytes32 => "varchar(64)",
            ColumnType::AssetId => "varchar(64)",
            ColumnType::ContractId => "varchar(64)",
            ColumnType::Salt => "varchar(64)",
            ColumnType::Int4 => "integer",
            ColumnType::Int8 => "bigint",
            ColumnType::UInt4 => "integer",
            ColumnType::UInt8 => "bigint",
            ColumnType::Timestamp => "timestamp",
            ColumnType::Blob => "bytea",
        }
    }
}

#[derive(Queryable, QueryableByName)]
#[table_name = "columns"]
#[allow(unused)]
pub struct Columns {
    pub id: i32,
    pub type_id: i64,
    pub column_position: i32,
    pub column_name: String,
    pub column_type: ColumnType,
    pub nullable: bool,
    pub graphql_type: String,
}

impl Columns {
    pub fn list_by_id(conn: &PgConnection, col_id: i64) -> QueryResult<Vec<Columns>> {
        use gr::columns::dsl::*;
        columns.filter(type_id.eq(col_id)).load(conn)
    }
}

#[derive(Debug, Queryable, QueryableByName)]
pub struct ColumnInfo {
    #[sql_type = "BigInt"]
    pub type_id: i64,
    #[sql_type = "Text"]
    pub table_name: String,
    #[sql_type = "Integer"]
    pub column_position: i32,
    #[sql_type = "Text"]
    pub column_name: String,
    #[sql_type = "Columntypename"]
    pub column_type: ColumnType,
}

impl ColumnInfo {
    pub fn get_schema(
        conn: &PgConnection,
        name: &str,
        version: &str,
    ) -> QueryResult<Vec<ColumnInfo>> {
        use gr::columns::dsl as cd;
        use gr::type_ids::dsl as td;

        let result = td::type_ids
            .inner_join(cd::columns.on(cd::type_id.eq(td::id)))
            .select((
                cd::type_id,
                td::table_name,
                cd::column_position,
                cd::column_name,
                cd::column_type,
            ))
            .filter(td::schema_name.eq(name).and(td::schema_version.eq(version)))
            .order((cd::type_id, cd::column_position))
            .load::<_>(conn)?;

        Ok(result)
    }
}

#[derive(Debug, QueryableByName)]
pub struct EntityData {
    #[sql_type = "Binary"]
    pub object: Vec<u8>,
}
