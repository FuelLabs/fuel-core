use crate::{IndexerConfig, SchemaManager};
use async_std::sync::{Arc, RwLock};
use fuel_indexer_schema::db::{graphql::GraphqlQueryBuilder, tables::Schema};
use log::error;
use serde::Deserialize;
use std::collections::HashMap;
use tokio_postgres::{connect, types::Type, NoTls};
use warp::{http::StatusCode, reply::with_status, Filter, Reply};

const MAX_JSON_BODY: u64 = 1024 * 16;

#[derive(Clone, Deserialize)]
struct Query {
    query: String,
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
            Ok(response) => {
                Ok(with_status(format!("{}", response), StatusCode::OK).into_response())
            }
            Err(e) => Ok(with_status(
                format!("Server Error {:?}", e),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .into_response()),
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

// TODO: proper error types for this api...
async fn run_query(
    query: Query,
    schema: Schema,
    config: Arc<IndexerConfig>,
) -> Result<String, tokio_postgres::Error> {
    let (client, conn) = connect(&config.database_url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            error!("Database connection error {:?}", e);
        }
    });

    let builder =
        GraphqlQueryBuilder::new(&schema, &query.query).expect("Error constructing builder");
    let query = builder.build().expect("Query builder failed");

    let queries = query.as_sql().join(";\n");
    let results = client.query(&queries, &[]).await?;

    // TODO: need a results formatter.
    let mut response = Vec::new();
    for row in results {
        response.push(HashMap::<String, serde_json::Value>::from_iter(
            row.columns()
                .iter()
                .map(|c| match c.type_() {
                    &Type::INT8 => (
                        c.name().to_string(),
                        serde_json::Value::Number(row.get::<&str, i64>(c.name()).into()),
                    ),
                    &Type::VARCHAR => (
                        c.name().to_string(),
                        serde_json::Value::String(row.get::<&str, String>(c.name())),
                    ),
                    t => panic!("Unhandled type! {}", t),
                })
                .collect::<Vec<(String, serde_json::Value)>>(),
        ));
    }

    Ok(serde_json::to_string(&response).expect("Could not serialize response"))
}
