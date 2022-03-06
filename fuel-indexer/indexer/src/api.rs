use crate::{IndexerConfig, SchemaManager};
use async_std::sync::{Arc, RwLock};
use axum::{
    extract::{Extension, Json, Path},
    routing::post,
    Router,
};
use fuel_indexer_schema::db::{
    graphql::{GraphqlError, GraphqlQueryBuilder},
    tables::Schema,
};
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tokio_postgres::{connect, types::Type, NoTls};
use tracing::error;

#[derive(Debug, Error)]
enum APIError {
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
}

#[derive(Clone, Debug, Deserialize)]
struct Query {
    query: String,
    #[allow(unused)] // TODO
    params: String,
}

async fn query_graph(
    Path(name): Path<String>,
    Json(query): Json<Query>,
    Extension(config): Extension<Arc<IndexerConfig>>,
    Extension(manager): Extension<Arc<RwLock<SchemaManager>>>,
) -> (StatusCode, Json<Value>) {
    match manager.read().await.load_schema(&name) {
        Ok(schema) => match run_query(query, schema, config).await {
            Ok(response) => (StatusCode::OK, Json(response)),
            Err(e) => {
                error!("Query error {e:?}");
                let res = Json(Value::String("Internal Server Error".into()));
                (StatusCode::INTERNAL_SERVER_ERROR, res)
            }
        },
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(Value::String(format!(
                "The graph {} was not found ({:?})",
                name, e
            ))),
        ),
    }
}

pub struct GraphQlApi;

impl GraphQlApi {
    pub async fn run(config: IndexerConfig) {
        let sm = SchemaManager::new(&config.database_url).expect("SchemaManager create failed");
        let schema_manager = Arc::new(RwLock::new(sm));
        let config = Arc::new(config);
        let listen_on = config.listen_endpoint;

        let app = Router::new()
            .route("/graph/:name", post(query_graph))
            .layer(Extension(schema_manager))
            .layer(Extension(config));

        axum::Server::bind(&listen_on)
            .serve(app.into_make_service())
            .await
            .expect("Service failed to start");
    }
}

async fn run_query(
    query: Query,
    schema: Schema,
    config: Arc<IndexerConfig>,
) -> Result<Value, APIError> {
    let (client, conn) = connect(&config.database_url, NoTls).await?;
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
