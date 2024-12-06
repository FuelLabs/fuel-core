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
        validation_extension::ValidationExtension,
        view_extension::ViewExtension,
        Config,
    },
    graphql_api::{
        self,
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
    http::GraphiQLSource,
    Request,
    Response,
};
use axum::{
    extract::{
        DefaultBodyLimit,
        Extension,
    },
    http::{
        self,
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
        },
        HeaderValue,
        StatusCode,
    },
    middleware::{
        self,
        Next,
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
    AsyncProcessor,
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::fuel_types::BlockHeight;
use futures::Stream;
use hyper::rt::Executor;
use serde_json::json;
use std::{
    future::Future,
    net::{
        SocketAddr,
        TcpListener,
    },
    pin::Pin,
    sync::Arc,
};
use tokio_stream::StreamExt;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub type Service = fuel_core_services::ServiceRunner<GraphqlService>;

pub use super::database::ReadDatabase;
use super::{
    ports::worker,
    worker_service::LAST_KNOWN_BLOCK_HEIGHT,
};

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
    number_of_threads: usize,
}

pub struct Task {
    // Ugly workaround because of https://github.com/hyperium/hyper/issues/2582
    server: Pin<Box<dyn Future<Output = hyper::Result<()>> + Send + 'static>>,
}

#[derive(Clone)]
struct ExecutorWithMetrics {
    processor: Arc<AsyncProcessor>,
}

impl<F> Executor<F> for ExecutorWithMetrics
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        let result = self.processor.try_spawn(fut);

        if let Err(err) = result {
            tracing::error!("Failed to spawn a task for GraphQL: {:?}", err);
        }
    }
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
        let ServerParams {
            router,
            listener,
            number_of_threads,
        } = params;

        let processor = AsyncProcessor::new(
            "GraphQLFutures",
            number_of_threads,
            tokio::sync::Semaphore::MAX_PERMITS,
        )?;

        let executor = ExecutorWithMetrics {
            processor: Arc::new(processor),
        };

        let server = axum::Server::from_tcp(listener)
            .unwrap()
            .executor(executor)
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
    async fn run(&mut self, _: &mut StateWatcher) -> TaskNextAction {
        match self.server.as_mut().await {
            Ok(()) => {
                // The `axum::Server` has its internal loop. If `await` is finished, we get an internal
                // error or stop signal.
                TaskNextAction::Stop
            }
            Err(err) => TaskNextAction::ErrorContinue(err.into()),
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        // The `axum::Server` was already gracefully shutdown at this point.
        Ok(())
    }
}

const REQUIRED_FUEL_BLOCK_HEIGHT_HEADER: &str = "REQUIRED_FUEL_BLOCK_HEIGHT";
const CURRENT_FUEL_BLOCK_HEIGHT_HEADER: &str = "CURRENT_FUEL_BLOCK_HEIGHT";

async fn required_fuel_block_height<Body>(
    req: http::Request<Body>,
    next: Next<Body>,
) -> impl IntoResponse {
    let last_known_block_height: BlockHeight = LAST_KNOWN_BLOCK_HEIGHT
    .get()
    //Maybe too strict?
    .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
    .load(std::sync::atomic::Ordering::Acquire)
    .into();

    let Some(required_fuel_block_height_header) = req
        .headers()
        .get(REQUIRED_FUEL_BLOCK_HEIGHT_HEADER)
        .map(|value| value.to_str())
    else {
        // Header is not present, so we don't have any requirements.
        let mut response = next.run(req).await;
        add_current_fuel_block_height_header_to_response(
            &mut response,
            &last_known_block_height,
        );

        return Ok(response);
    };

    let raw_required_fuel_block_height =
        required_fuel_block_height_header.map_err(|_err| StatusCode::BAD_REQUEST)?;

    let required_fuel_block_height: BlockHeight = raw_required_fuel_block_height
        .parse::<u32>()
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .into();

    if required_fuel_block_height > last_known_block_height {
        Err(StatusCode::PRECONDITION_FAILED)
    } else {
        let mut response = next.run(req).await;
        add_current_fuel_block_height_header_to_response(
            &mut response,
            &last_known_block_height,
        );

        Ok(response)
    }
}

fn add_current_fuel_block_height_header_to_response<Body>(
    response: &mut http::Response<Body>,
    last_known_block_height: &BlockHeight,
) {
    response.headers_mut().insert(
        CURRENT_FUEL_BLOCK_HEIGHT_HEADER,
        HeaderValue::from_str(&last_known_block_height.to_string()).unwrap(),
    );
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
    OffChain: AtomicView + worker::OffChainDatabase + 'static,
    OnChain::LatestView: OnChainDatabase,
    OffChain::LatestView: OffChainDatabase,
{
    graphql_api::initialize_query_costs(config.config.costs.clone())?;

    let network_addr = config.config.addr;
    let combined_read_database = ReadDatabase::new(
        config.config.database_batch_size,
        genesis_block_height,
        on_database,
        off_database,
    )?;
    let request_timeout = config.config.api_request_timeout;
    let concurrency_limit = config.config.max_concurrent_queries;
    let body_limit = config.config.request_body_bytes_limit;
    let max_queries_resolver_recursive_depth =
        config.config.max_queries_resolver_recursive_depth;
    let number_of_threads = config.config.number_of_threads;

    let schema = schema
        .limit_complexity(config.config.max_queries_complexity)
        .limit_depth(config.config.max_queries_depth)
        .limit_recursive_depth(config.config.max_queries_recursive_depth)
        .limit_directives(config.config.max_queries_directives)
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
        .extension(ValidationExtension::new(
            max_queries_resolver_recursive_depth,
        ))
        .extension(async_graphql::extensions::Tracing)
        .extension(ViewExtension::new())
        .finish();

    let graphql_endpoint = "/v1/graphql";
    let graphql_subscription_endpoint = "/v1/graphql-sub";

    let graphql_playground =
        || render_graphql_playground(graphql_endpoint, graphql_subscription_endpoint);

    let router = Router::new()
        .route("/v1/playground", get(graphql_playground))
        .route(
            graphql_endpoint,
            post(graphql_handler)
                .layer(middleware::from_fn(required_fuel_block_height))
                .layer(ConcurrencyLimitLayer::new(concurrency_limit))
                .options(ok),
        )
        .route(
            graphql_subscription_endpoint,
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
        ServerParams {
            router,
            listener,
            number_of_threads,
        },
    ))
}

async fn render_graphql_playground(
    endpoint: &str,
    subscription_endpoint: &str,
) -> impl IntoResponse {
    Html(
        GraphiQLSource::build()
            .endpoint(endpoint)
            .subscription_endpoint(subscription_endpoint)
            .title("Fuel Graphql Playground")
            .finish(),
    )
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
