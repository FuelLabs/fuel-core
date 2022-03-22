use async_std::{net::SocketAddr, sync::{Arc, RwLock}};
use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
    routing::post,
    Router,
};
use fuel_indexer_schema::graphql::{
    GraphqlError,
    Schema,
    table_name,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use tracing::error;

#[cfg(feature = "db-postgres")]
mod db_postgres;

#[cfg(feature = "db-postgres")]
use db_postgres::{run_query, load_schema};

#[cfg(feature = "db-sqlite")]
mod db_sqlite;

#[cfg(feature = "db-sqlite")]
use db_sqlite::{run_query, load_schema};


#[derive(Debug, Error)]
pub enum APIError {
    #[cfg(feature = "db-sqlite")]
    #[error("Sqlite error {0:?}")]
    SqliteError(#[from] sqlite::Error),
    #[cfg(feature = "db-postgres")]
    #[error("Postgres error {0:?}")]
    PostgresError(#[from] tokio_postgres::Error),
    #[error("Query builder error {0:?}")]
    GraphqlError(#[from] GraphqlError),
    #[error("Malformed query")]
    MalformedQuery,
    #[error("Unexpected DB type {0:?}")]
    UnexpectedDBType(String),
    #[error("Serde Error {0:?}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Graph Not Found.")]
    GraphNotFound,
}

fn root_query(namespace: &str) -> String {
    let table = table_name("graph_registry", "graph_root");

    format!(r#"SELECT id,version,query,schema
    FROM {table} 
    WHERE schema_name = '{namespace}'
    ORDER BY id DESC
    LIMIT 1"#)
}

fn root_columns(root_id: &str) -> String {
    let table = table_name("graph_registry", "root_columns");

    format!(r#"SELECT id,column_name,graphql_type
    FROM {table}
    WHERE root_id = {root_id}"#)
}

fn graph_types(name: &str, version: &str) -> String {
    let table1 = table_name("graph_registry", "type_ids");
    let table2 = table_name("graph_registry", "columns");

    format!(r#"SELECT tid.graphql_name,col.column_name,col.graphql_type
    FROM {table1} tid
    JOIN {table2} col
    ON tid.id = col.type_id
    WHERE tid.schema_name = '{name}'
    AND tid.schema_version = '{version}'"#)
}


#[derive(Clone, Debug, Deserialize)]
pub struct Query {
    query: String,
    #[allow(unused)] // TODO
    params: String,
}

type SchemaManager = HashMap<String, Schema>;

async fn query_graph(
    Path(name): Path<String>,
    Json(query): Json<Query>,
    Extension(database_url): Extension<String>,
    Extension(manager): Extension<Arc<RwLock<SchemaManager>>>,
) -> (StatusCode, Json<Value>) {
    if !manager.read().await.contains_key(&name) {
        if let Ok(Some(schema)) = load_schema(&database_url, &name).await {
            manager.write().await.insert(name.clone(), schema);
        } else {
            let result = format!("The graph {name} was not found.");
            return (StatusCode::NOT_FOUND, Json(Value::String(result)));
        }
    };

    let guard = manager.read().await;
    let schema = guard.get(&name).unwrap();

    match run_query(query, schema, database_url).await {
        Ok(response) => (StatusCode::OK, Json(response)),
        Err(e) => {
            error!("Query error {e:?}");
            let res = Json(Value::String("Internal Server Error".into()));
            (StatusCode::INTERNAL_SERVER_ERROR, res)
        }
    }
}

pub struct GraphQlApi {
    database_url: String,
    listen_address: SocketAddr,
}

impl GraphQlApi {
    pub fn new(database_url: String, listen_address: SocketAddr) -> GraphQlApi {
        GraphQlApi {
            database_url,
            listen_address,
        }
    }

    pub async fn run(self) {
        let sm = SchemaManager::new();
        let schema_manager = Arc::new(RwLock::new(sm));

        let app = Router::new()
            .route("/graph/:name", post(query_graph))
            .layer(Extension(self.database_url.clone()))
            .layer(Extension(schema_manager));

        axum::Server::bind(&self.listen_address)
            .serve(app.into_make_service())
            .await
            .expect("Service failed to start");
    }
}
