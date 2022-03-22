use crate::{graph_types, root_columns, root_query, APIError, Query};
use fuel_indexer_schema::graphql::{GraphqlQueryBuilder, Schema};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tokio_postgres::{connect, types::Type, NoTls};
use tracing::error;

pub async fn load_schema(database_url: &str, name: &str) -> Result<Option<Schema>, APIError> {
    let (client, conn) = connect(database_url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            error!("Database connection error {:?}", e);
        }
    });
    let mut types = HashSet::new();
    let mut fields = HashMap::new();

    let query = root_query(name);
    let results = client.query(&query, &[]).await?;

    if results.len() == 0 {
        return Err(APIError::GraphNotFound);
    }

    let row = &results[0];

    let (root_id, schema_version, query_root) = {
        let id = row.get::<usize, i32>(0);
        let version = row.get::<usize, String>(1);
        let root = row.get::<usize, String>(2);
        (id, version, root)
    };
    types.insert(query_root.clone());

    let query = root_columns(&format!("{}", root_id));
    let results = client.query(&query, &[]).await?;

    let mut cols = HashMap::new();
    for row in results {
        let name = row.get::<usize, String>(1);
        let typ = row.get::<usize, String>(2);
        cols.insert(name, typ);
    }

    fields.insert(query_root.clone(), cols);

    let query = graph_types(name, &schema_version);
    let results = client.query(&query, &[]).await?;

    for row in results {
        let name = row.get::<usize, String>(0);
        let column = row.get::<usize, String>(1);
        let graphql_type = row.get::<usize, String>(2);

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
    let (client, conn) = connect(&database_url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            error!("Database connection error {:?}", e);
        }
    });

    let builder = GraphqlQueryBuilder::new(&schema, &query.query)?;
    let query = builder.build()?;

    let queries = query.as_sql(true).join(";\n");
    let results = client.query(&queries, &[]).await?;

    let mut response = Vec::new();
    for row in results {
        if row.columns().len() != 1 {
            return Err(APIError::MalformedQuery);
        }
        let c = &row.columns()[0];
        let value = match c.type_() {
            &Type::JSON => row.get::<&str, Value>(c.name()),
            t => return Err(APIError::UnexpectedDBType(format!("{t:?}"))),
        };

        response.push(value);
    }

    Ok(Value::Array(response))
}
