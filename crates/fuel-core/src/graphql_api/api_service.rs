use crate::{
    fuel_core_graphql_api::{
        metrics_extension::MetricsExtension,
        ports::{
            BlockProducerPort,
            ConsensusModulePort,
            ConsensusProvider as ConsensusProviderTrait,
            GasPriceEstimate,
            OffChainDatabase,
            OnChainDatabase,
            P2pPort,
            TxPoolPort,
        },
        view_extension::ViewExtension,
        Config,
    },
    schema::{
        CoreSchema,
        CoreSchemaBuilder,
    },
    service::{
        adapters::SharedMemoryPool,
        metrics::metrics,
    },
};
use async_graphql::{
    http::{
        playground_source,
        GraphQLPlaygroundConfig,
    },
    Request,
    Response,
};
use axum::{
    extract::{
        DefaultBodyLimit,
        Extension,
    },
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
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::fuel_types::BlockHeight;
use futures::Stream;
use serde_json::json;
use std::{
    future::Future,
    net::{
        SocketAddr,
        TcpListener,
    },
    pin::Pin,
};
use tokio_stream::StreamExt;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub type Service = fuel_core_services::ServiceRunner<GraphqlService>;

pub use super::database::ReadDatabase;

pub type BlockProducer = Box<dyn BlockProducerPort>;
// In the future GraphQL should not be aware of `TxPool`. It should
//  use only `Database` to receive all information about transactions.
pub type TxPool = Box<dyn TxPoolPort>;
pub type ConsensusModule = Box<dyn ConsensusModulePort>;
pub type P2pService = Box<dyn P2pPort>;

pub type GasPriceProvider = Box<dyn GasPriceEstimate>;

pub type ConsensusProvider = Box<dyn ConsensusProviderTrait>;

#[derive(Clone)]
pub struct SharedState {
    pub bound_address: SocketAddr,
}

pub struct GraphqlService {
    bound_address: SocketAddr,
}

pub struct ServerParams {
    router: Router,
    listener: TcpListener,
}

pub struct Task {
    // Ugly workaround because of https://github.com/hyperium/hyper/issues/2582
    server: Pin<Box<dyn Future<Output = hyper::Result<()>> + Send + 'static>>,
}

#[async_trait::async_trait]
impl RunnableService for GraphqlService {
    const NAME: &'static str = "GraphQL";

    type SharedData = SharedState;
    type Task = Task;
    type TaskParams = ServerParams;

    fn shared_data(&self) -> Self::SharedData {
        SharedState {
            bound_address: self.bound_address,
        }
    }

    async fn into_task(
        self,
        state: &StateWatcher,
        params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let mut state = state.clone();
        let ServerParams { router, listener } = params;

        let server = axum::Server::from_tcp(listener)
            .unwrap()
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                state
                    .while_started()
                    .await
                    .expect("The service is destroyed");
            });

        Ok(Task {
            server: Box::pin(server),
        })
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    async fn run(&mut self, _: &mut StateWatcher) -> anyhow::Result<bool> {
        self.server.as_mut().await?;
        // The `axum::Server` has its internal loop. If `await` is finished, we get an internal
        // error or stop signal.
        Ok(false /* should_continue */)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        // The `axum::Server` was already gracefully shutdown at this point.
        Ok(())
    }
}

// Need a separate Data Object for each Query endpoint, cannot be avoided
#[allow(clippy::too_many_arguments)]
pub fn new_service<OnChain, OffChain>(
    genesis_block_height: BlockHeight,
    config: Config,
    schema: CoreSchemaBuilder,
    on_database: OnChain,
    off_database: OffChain,
    txpool: TxPool,
    producer: BlockProducer,
    consensus_module: ConsensusModule,
    p2p_service: P2pService,
    gas_price_provider: GasPriceProvider,
    consensus_parameters_provider: ConsensusProvider,
    memory_pool: SharedMemoryPool,
) -> anyhow::Result<Service>
where
    OnChain: AtomicView + 'static,
    OffChain: AtomicView + 'static,
    OnChain::LatestView: OnChainDatabase,
    OffChain::LatestView: OffChainDatabase,
{
    let network_addr = config.config.addr;
    let combined_read_database =
        ReadDatabase::new(genesis_block_height, on_database, off_database);
    let request_timeout = config.config.api_request_timeout;
    let body_limit = config.config.request_body_bytes_limit;

    let schema = schema
        .limit_complexity(config.config.max_queries_complexity)
        .limit_depth(config.config.max_queries_depth)
        .limit_recursive_depth(config.config.max_queries_recursive_depth)
        .extension(MetricsExtension::new(
            config.config.query_log_threshold_time,
        ))
        .data(config)
        .data(combined_read_database)
        .data(txpool)
        .data(producer)
        .data(consensus_module)
        .data(p2p_service)
        .data(gas_price_provider)
        .data(consensus_parameters_provider)
        .data(memory_pool)
        .extension(async_graphql::extensions::Tracing)
        .extension(ViewExtension::new())
        .finish();

    let router = Router::new()
        .route("/v1/playground", get(graphql_playground))
        .route("/v1/graphql", post(graphql_handler).options(ok))
        .route(
            "/v1/graphql-sub",
            post(graphql_subscription_handler).options(ok),
        )
        .route("/v1/metrics", get(metrics))
        .route("/v1/health", get(health))
        .route("/health", get(health))
        .layer(Extension(schema))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(request_timeout))
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
        ))
        .layer(DefaultBodyLimit::max(body_limit));

    let listener = TcpListener::bind(network_addr)?;
    let bound_address = listener.local_addr()?;

    tracing::info!("Binding GraphQL provider to {}", bound_address);

    Ok(Service::new_with_params(
        GraphqlService { bound_address },
        ServerParams { router, listener },
    ))
}

async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new(
        "/v1/graphql",
    )))
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
        .map(|r| Event::default().json_data(r));
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("keep-alive-text"))
}

async fn ok() -> anyhow::Result<(), ()> {
    Ok(())
}
