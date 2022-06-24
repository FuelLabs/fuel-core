use super::modules::Modules;
use crate::database::Database;
use crate::schema::{build_schema, dap, CoreSchema};
use crate::service::metrics::metrics;
use crate::service::Config;
use anyhow::Result;
use async_graphql::{
    extensions::Tracing, http::playground_source, http::GraphQLPlaygroundConfig, Request, Response,
};
use axum::{
    extract::Extension,
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
        },
        HeaderValue,
    },
    response::Html,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::net::{SocketAddr, TcpListener};
use tokio::task::JoinHandle;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};
use tracing::info;

/// Spawns the api server for this node
pub async fn start_server(
    config: Config,
    db: Database,
    modules: &Modules,
) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
    let network_addr = config.addr;
    let params = config.chain_conf.transaction_parameters;
    let schema = build_schema()
        .data(config)
        .data(db)
        .data(modules.txpool.clone())
        .data(modules.block_importer.clone())
        .data(modules.block_producer.clone())
        .data(modules.sync.clone())
        .data(modules.bft.clone());
    let schema = dap::init(schema, params).extension(Tracing).finish();

    let router = Router::new()
        .route("/playground", get(graphql_playground))
        .route("/graphql", post(graphql_handler).options(ok))
        .route("/metrics", get(metrics))
        .route("/health", get(health))
        .layer(Extension(schema))
        .layer(TraceLayer::new_for_http())
        .layer(SetResponseHeaderLayer::<_>::overriding(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        ))
        .layer(SetResponseHeaderLayer::<_>::overriding(
            ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("*"),
        ))
        .layer(SetResponseHeaderLayer::<_>::overriding(
            ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("*"),
        ));

    let (tx, rx) = tokio::sync::oneshot::channel();
    let listener = TcpListener::bind(&network_addr)?;
    let bound_addr = listener.local_addr().unwrap();

    info!("Binding GraphQL provider to {}", bound_addr);
    let handle = tokio::spawn(async move {
        let server = axum::Server::from_tcp(listener)
            .unwrap()
            .serve(router.into_make_service());

        tx.send(()).unwrap();
        server.await.map_err(Into::into)
    });

    // wait until the server is ready
    rx.await.unwrap();

    Ok((bound_addr, handle))
}

async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "up": true }))
}

async fn graphql_handler(schema: Extension<CoreSchema>, req: Json<Request>) -> Json<Response> {
    schema.execute(req.0).await.into()
}

async fn ok() -> Result<(), ()> {
    Ok(())
}
