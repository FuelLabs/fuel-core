use crate::database::Database;
use crate::schema::{build_schema, dap, CoreSchema};
use crate::tx_pool::TxPool;
use async_graphql::{http::playground_source, http::GraphQLPlaygroundConfig, Request, Response};
use axum::{
    body::Body,
    extract::Extension,
    handler::{get, post},
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
        },
        HeaderValue,
    },
    response::Html,
    response::IntoResponse,
    routing::BoxRoute,
    AddExtensionLayer, Json, Router,
};
use serde_json::json;
use std::{
    net,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::Arc,
};
use strum_macros::{Display, EnumString};
use tower_http::set_header::SetResponseHeaderLayer;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: net::SocketAddr,
    pub database_path: PathBuf,
    pub database_type: DbType,
}

#[derive(Clone, Debug, PartialEq, EnumString, Display)]
pub enum DbType {
    InMemory,
    RocksDb,
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

pub fn configure(db: Database) -> Router<BoxRoute> {
    let tx_pool = Arc::new(TxPool::new(db.clone()));
    let schema = build_schema().data(db).data(tx_pool);
    let schema = dap::init(schema).finish();

    Router::new()
        .route("/playground", get(graphql_playground))
        .route("/graphql", post(graphql_handler).options(ok))
        .route("/health", get(health))
        .layer(AddExtensionLayer::new(schema))
        .layer(SetResponseHeaderLayer::<_, Body>::overriding(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        ))
        .layer(SetResponseHeaderLayer::<_, Body>::overriding(
            ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("*"),
        ))
        .layer(SetResponseHeaderLayer::<_, Body>::overriding(
            ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("*"),
        ))
        .boxed()
}

/// Run a `BoxRoute` service in the background and get a URI for it.
pub async fn run_in_background(svc: Router<BoxRoute>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Could not bind ephemeral socket");
    let addr = listener.local_addr().unwrap();
    println!("Listening on {}", addr);

    let (tx, rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let server = axum::Server::from_tcp(listener)
            .unwrap()
            .serve(svc.into_make_service());
        tx.send(()).unwrap();
        server.await.expect("server error");
    });

    rx.await.unwrap();

    addr
}
