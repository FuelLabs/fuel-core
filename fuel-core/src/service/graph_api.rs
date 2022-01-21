use crate::database::Database;
use crate::schema::{build_schema, dap, CoreSchema};
use crate::service::Config;
use crate::tx_pool::TxPool;
use async_graphql::{http::playground_source, http::GraphQLPlaygroundConfig, Request, Response};
use axum::routing::{get, post};
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
    AddExtensionLayer, Json, Router,
};
use serde_json::json;
use std::{
    net::{SocketAddr, TcpListener},
    sync::Arc,
};
use tokio::task::JoinHandle;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::info;

/// Spawns the api server for this node
pub async fn start_server(
    config: Config,
    db: Database,
    tx_pool: Arc<TxPool>,
) -> Result<(SocketAddr, JoinHandle<Result<(), crate::service::Error>>), std::io::Error> {
    let schema = build_schema().data(db).data(tx_pool);
    let schema = dap::init(schema).finish();

    let router = Router::new()
        .route("/playground", get(graphql_playground))
        .route("/graphql", post(graphql_handler).options(ok))
        .route("/health", get(health))
        .layer(AddExtensionLayer::new(schema))
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
    let listener = TcpListener::bind(&config.addr)?;
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
