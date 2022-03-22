use crate::{graph_types, root_columns, root_query, APIError, Query};
use fuel_indexer_schema::graphql::{GraphqlQueryBuilder, Schema};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub async fn load_schema(database_url: &str, name: &str) -> Result<Option<Schema>, APIError> {
    let mut types = HashSet::new();
    let mut fields = HashMap::new();

    let query = root_query(name);
    let conn = sqlite::open(database_url)?;
    let mut statement = conn.prepare(query)?;

    let (root_id, schema_version, query_root) = if let sqlite::State::Row = statement.next()? {
        let id = statement.read::<String>(0)?;
        let version = statement.read::<String>(1)?;
        let root = statement.read::<String>(2)?;
        (id, version, root)
    } else {
        return Err(APIError::GraphNotFound);
    };
    types.insert(query_root.clone());

    let query = root_columns(&root_id);
    let mut statement = conn.prepare(query)?;

    let mut cols = HashMap::new();
    while let sqlite::State::Row = statement.next()? {
        let name = statement.read::<String>(1)?;
        let typ = statement.read::<String>(2)?;
        cols.insert(name, typ);
    }

    fields.insert(query_root.clone(), cols);

    let query = graph_types(name, &schema_version);

    let mut statement = conn.prepare(query)?;

    while let sqlite::State::Row = statement.next()? {
        let name = statement.read::<String>(0)?;
        let column = statement.read::<String>(1)?;
        let graphql_type = statement.read::<String>(2)?;

        types.insert(name.clone());
        let entry = fields.entry(name).or_default();
        entry.insert(column, graphql_type);
    }

    Ok(Some(Schema {
        version: schema_version,
        namespace: name.into(),
        query: query_root,
        types,
        fields,
    }))
}

pub async fn run_query(
    query: Query,
    schema: &Schema,
    database_url: String,
) -> Result<Value, APIError> {
    let conn = sqlite::open(database_url)?;
    let builder = GraphqlQueryBuilder::new(&schema, &query.query)?;
    let query = builder.build()?;

    let queries = query.as_sql(true).join(";\n");
    let mut statement = conn.prepare(queries)?;

    while let sqlite::State::Row = statement.next()? {
        let row: serde_json::Value = serde_json::from_str(&statement.read::<String>(0)?)?;
        return Ok(row);
    }
    Ok(Value::Null)
}
