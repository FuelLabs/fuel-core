use crate::database::{Database, DatabaseTrait};
use crate::schema::{build_schema, dap, CoreSchema};
use async_graphql::{http::playground_source, http::GraphQLPlaygroundConfig};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    body::Body,
    prelude::{response::IntoResponse, RoutingDsl, *},
    routing::BoxRoute,
    AddExtensionLayer,
};
use std::net::{SocketAddr, TcpListener};
use std::{net, path::PathBuf, sync::Arc};
use strum_macros::Display;
use strum_macros::EnumString;

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

#[derive(Clone, Debug)]
pub struct SharedDatabase(pub Arc<dyn DatabaseTrait + Send + Sync>);

impl Default for SharedDatabase {
    fn default() -> Self {
        SharedDatabase(Arc::new(Database::default()))
    }
}

async fn graphql_playground() -> impl IntoResponse {
    response::Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

async fn graphql_handler(
    schema: extract::Extension<CoreSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

pub fn configure(db: SharedDatabase) -> BoxRoute<Body> {
    let schema = build_schema().data(db);
    let schema = dap::init(schema).finish();

    route("/playground", get(graphql_playground))
        .route("/graphql", post(graphql_handler))
        .layer(AddExtensionLayer::new(schema))
        .boxed()
}

/// Run a `BoxRoute` service in the background and get a URI for it.
pub async fn run_in_background(svc: BoxRoute<Body>) -> SocketAddr {
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
