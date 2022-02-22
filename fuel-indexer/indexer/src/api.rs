use crate::{IndexerConfig, SchemaManager};
use async_std::sync::{Arc, RwLock};
use fuel_indexer_schema::db::{
    graphql::{GraphqlError, GraphqlQueryBuilder},
    tables::Schema,
};
use serde::Deserialize;
use thiserror::Error;
use tokio_postgres::{connect, types::Type, NoTls};
use tracing::error;
use warp::{http::StatusCode, reply::with_status, Filter, Reply};

const MAX_JSON_BODY: u64 = 1024 * 16;

#[derive(Debug, Error)]
enum APIError {
    #[error("Postgres error {0:?}")]
    PostgresError(#[from] tokio_postgres::Error),
    #[error("Query builder error {0:?}")]
    GraphqlError(#[from] GraphqlError),
    #[error("Unexpected DB type {0:?}")]
    UnexpectedDBType(String),
    #[error("Unexpected JSON type {0:?}")]
    UnexpectedJSONType(String),
    #[error("Serde Error {0:?}")]
    SerdeError(#[from] serde_json::Error),
}

#[derive(Clone, Deserialize)]
struct Query {
    query: String,
    #[allow(unused)] // TODO
    params: String,
}

fn extract_json() -> impl Filter<Extract = (Query,), Error = warp::Rejection> + Clone {
    warp::body::content_length_limit(MAX_JSON_BODY).and(warp::body::json())
}

async fn query_graph(
    name: String,
    query: Query,
    manager: Arc<RwLock<SchemaManager>>,
    config: Arc<IndexerConfig>,
) -> Result<impl warp::Reply, warp::Rejection> {
    match manager.read().await.load_schema(&name) {
        Ok(schema) => match run_query(query, schema, config).await {
            Ok(response) => Ok(with_status(response, StatusCode::OK).into_response()),
            Err(e) => {
                error!("Query error {e:?}");
                Ok(with_status(
                    "Internal Server Error".to_string(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response())
            }
        },
        Err(e) => Ok(with_status(
            format!("The graph {} was not found ({:?})", name, e),
            StatusCode::NOT_FOUND,
        )
        .into_response()),
    }
}

pub struct GraphQlAPI;

impl GraphQlAPI {
    pub async fn run(config: IndexerConfig) {
        let sm = SchemaManager::new(&config.database_url).expect("SchemaManager create failed");
        let schema_manager = Arc::new(RwLock::new(sm));
        let config = Arc::new(config);
        let mgr_filter = warp::any().map(move || schema_manager.clone());

        let a_config = config.clone();
        let rquery = warp::post()
            .and(warp::path("graph"))
            .and(warp::path::param())
            .and(extract_json())
            .and(mgr_filter.clone())
            .and_then(
                move |name: String, query: Query, manager: Arc<RwLock<SchemaManager>>| {
                    let cfg = a_config.clone();
                    query_graph(name, query, manager, cfg)
                },
            );

        warp::serve(rquery).run(config.listen_endpoint).await
    }
}

async fn run_query(
    query: Query,
    schema: Schema,
    config: Arc<IndexerConfig>,
) -> Result<String, APIError> {
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
        assert_eq!(row.columns().len(), 1);
        let c = &row.columns()[0];
        let value = match c.type_() {
            &Type::JSON => row.get::<&str, serde_json::Value>(c.name()),
            t => return Err(APIError::UnexpectedDBType(format!("{t:?}"))),
        };

        if let serde_json::Value::Object(obj) = value {
            response.push(obj.clone());
        } else {
            return Err(APIError::UnexpectedJSONType(format!("{value:?}")));
        }
    }

    Ok(serde_json::to_string(&response)?)
}
