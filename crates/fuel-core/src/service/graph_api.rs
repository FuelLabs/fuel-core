use super::modules::Modules;
use crate::{
    database::Database,
    schema::{
        build_schema,
        dap,
        CoreSchema,
    },
    service::{
        metrics::metrics,
        Config,
    },
};
use async_graphql::{
    extensions::Tracing,
    http::{
        playground_source,
        GraphQLPlaygroundConfig,
    },
    Request,
    Response,
};
use axum::{
    extract::Extension,
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
        },
        HeaderValue,
    },
    response::{
        sse::Event,
        Html,
        IntoResponse,
        Sse,
    },
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use futures::Stream;
use serde_json::json;
use std::net::{
    SocketAddr,
    TcpListener,
};
use tokio::{
    sync::oneshot,
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use tracing::info;

/// Spawns the api server for this node
pub async fn start_server(
    config: Config,
    db: Database,
    modules: &Modules,
    stop: oneshot::Receiver<()>,
) -> anyhow::Result<(SocketAddr, JoinHandle<anyhow::Result<()>>)> {
    let network_addr = config.addr;
    let params = config.chain_conf.transaction_parameters;
    let schema = build_schema()
        .data(config)
        .data(db)
        .data(modules.txpool.clone())
        .data(modules.block_producer.clone())
        .data(modules.consensus_module.clone());
    let schema = dap::init(schema, params).extension(Tracing).finish();

    let router = Router::new()
        .route("/playground", get(graphql_playground))
        .route("/graphql", post(graphql_handler).options(ok))
        .route(
            "/graphql-sub",
            post(graphql_subscription_handler).options(ok),
        )
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
    let listener = TcpListener::bind(network_addr)?;
    let bound_addr = listener.local_addr().unwrap();

    info!("Binding GraphQL provider to {}", bound_addr);
    let handle = tokio::spawn(async move {
        let server = axum::Server::from_tcp(listener)
            .unwrap()
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                let _ = stop.await;
            });

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

async fn graphql_handler(
    schema: Extension<CoreSchema>,
    req: Json<Request>,
) -> Json<Response> {
    schema.execute(req.0).await.into()
}

async fn graphql_subscription_handler(
    schema: Extension<CoreSchema>,
    req: Json<Request>,
) -> Sse<impl Stream<Item = anyhow::Result<Event, serde_json::Error>>> {
    let stream = schema
        .execute_stream(req.0)
        .map(|r| Ok(Event::default().json_data(r).unwrap()));
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("keep-alive-text"))
}

async fn ok() -> anyhow::Result<(), ()> {
    Ok(())
}
