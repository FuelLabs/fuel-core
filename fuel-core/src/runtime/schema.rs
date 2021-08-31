use diesel::sql_types::*;
use diesel::*;

// TODO: need migration for this....
#[derive(Debug, QueryableByName)]
pub struct ColumnInfo {
    #[sql_type = "BigInt"]
    pub type_id: i64,
    #[sql_type = "Text"]
    pub table_name: String,
    #[sql_type = "Integer"]
    pub column_position: i32,
    #[sql_type = "Text"]
    pub column_name: String,
    #[sql_type = "Integer"]
    pub column_type: i32,
}

#[derive(Debug, QueryableByName)]
pub struct EntityData {
    #[sql_type = "Binary"]
    pub object: Vec<u8>,
}

#[derive(Debug, QueryableByName)]
pub struct RowCount {
    #[sql_type = "BigInt"]
    pub count: i64,
}
