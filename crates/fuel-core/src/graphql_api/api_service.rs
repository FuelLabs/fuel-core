use crate::{
    fuel_core_graphql_api::{
        Config,
        extensions::unify_response,
        ports::{
            BlockProducerPort,
            ChainStateProvider as ChainStateProviderTrait,
            ConsensusModulePort,
            GasPriceEstimate,
            OffChainDatabase,
            OffChainDatabaseAt,
            OnChainDatabase,
            P2pPort,
            TxPoolPort,
            TxStatusManager,
        },
    },
    graphql_api::{
        self,
        extensions::{
            chain_state_info::ChainStateInfoExtension,
            metrics::MetricsExtension,
            required_fuel_block_height::RequiredFuelBlockHeightExtension,
            validation::ValidationExtension,
        },
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
    Request,
    Response,
    http::GraphiQLSource,
};
use axum::{
    Json,
    Router,
    extract::{
        DefaultBodyLimit,
        Extension,
    },
    http::{
        HeaderValue,
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
        },
    },
    response::{
        Html,
        IntoResponse,
        Sse,
        sse::Event,
    },
    routing::{
        get,
        post,
    },
};
use fuel_core_services::{
    AsyncProcessor,
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_storage::transactional::HistoricalView;
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
    sync::{
        Arc,
        OnceLock,
    },
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
    block_height_subscription,
    ports::{
        DatabaseDaCompressedBlocks,
        OnChainDatabaseAt,
        worker,
    },
};

pub type BlockProducer = Box<dyn BlockProducerPort>;
// In the future GraphQL should not be aware of `TxPool`. It should
//  use only `Database` to receive all information about transactions.
pub type TxPool = Box<dyn TxPoolPort>;
pub type DynTxStatusManager = Box<dyn TxStatusManager>;
pub type ConsensusModule = Box<dyn ConsensusModulePort>;
pub type P2pService = Box<dyn P2pPort>;

pub type GasPriceProvider = Box<dyn GasPriceEstimate>;

pub type ChainInfoProvider = Box<dyn ChainStateProviderTrait>;

pub type DaCompressionProvider = Box<dyn DatabaseDaCompressedBlocks>;

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

// Need a separate Data Object for each Query endpoint, cannot be avoided
#[allow(clippy::too_many_arguments)]
pub fn new_service<OnChain, OffChain>(
    genesis_block_height: BlockHeight,
    config: Config,
    schema: CoreSchemaBuilder,
    on_database: OnChain,
    off_database: OffChain,
    txpool: TxPool,
    tx_status_manager: DynTxStatusManager,
    producer: BlockProducer,
    consensus_module: ConsensusModule,
    p2p_service: P2pService,
    gas_price_provider: GasPriceProvider,
    chain_state_info_provider: ChainInfoProvider,
    memory_pool: SharedMemoryPool,
    block_height_subscriber: block_height_subscription::Subscriber,
    da_compression_provider: DaCompressionProvider,
) -> anyhow::Result<Service>
where
    OnChain: HistoricalView<Height = BlockHeight> + 'static,
    OffChain: HistoricalView<Height = BlockHeight> + worker::OffChainDatabase + 'static,
    OnChain::LatestView: OnChainDatabase,
    OffChain::LatestView: OffChainDatabase,
    OnChain::ViewAtHeight: OnChainDatabaseAt,
    OffChain::ViewAtHeight: OffChainDatabaseAt,
{
    let balances_indexation_enabled = off_database.balances_indexation_enabled()?;

    let mut cost_config = config.config.costs;

    if !balances_indexation_enabled {
        cost_config.balance_query = graphql_api::BALANCES_QUERY_COST_WITHOUT_INDEXATION;
    }

    graphql_api::initialize_query_costs(cost_config, balances_indexation_enabled)?;

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
    let required_fuel_block_height_tolerance =
        config.config.required_fuel_block_height_tolerance;
    let required_fuel_block_height_timeout =
        config.config.required_fuel_block_height_timeout;

    let schema = schema
        .limit_complexity(config.config.max_queries_complexity)
        .limit_depth(config.config.max_queries_depth)
        .limit_recursive_depth(config.config.max_queries_recursive_depth)
        .limit_directives(config.config.max_queries_directives)
        // The ordering for extensions meters, the `ChainStateInfoExtension` should be the
        // first, because it adds additional information to the final response.
        .extension(ChainStateInfoExtension::new(block_height_subscriber.clone()))
        .extension(MetricsExtension::new(
            config.config.query_log_threshold_time,
        ))
        .data(config)
        .data(combined_read_database)
        .data(txpool)
        .data(tx_status_manager)
        .data(producer)
        .data(consensus_module)
        .data(p2p_service)
        .data(gas_price_provider)
        .data(chain_state_info_provider)
        .data(memory_pool)
        .data(da_compression_provider)
        .data(block_height_subscriber.clone())
        .extension(ValidationExtension::new(
            max_queries_resolver_recursive_depth,
        ))
        .extension(async_graphql::extensions::Tracing)
        .extension(RequiredFuelBlockHeightExtension::new(
            required_fuel_block_height_tolerance,
            required_fuel_block_height_timeout,
            block_height_subscriber,
        ))
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

/// Single initialization of the GraphQL playground HTML.
/// This is because the rendering and replacing is expensive
static GRAPHQL_PLAYGROUND_HTML: OnceLock<Arc<String>> = OnceLock::new();

fn _render_graphql_playground(
    endpoint: &str,
    subscription_endpoint: &str,
) -> impl IntoResponse + Send + Sync {
    let html = GRAPHQL_PLAYGROUND_HTML.get_or_init(|| {
        let raw_html = GraphiQLSource::build()
            .endpoint(endpoint)
            .subscription_endpoint(subscription_endpoint)
            .title("Fuel Graphql Playground")
            .finish();

        // this may not be necessary in the future,
        // but we need it to patch: https://github.com/async-graphql/async-graphql/issues/1703
        let raw_html = raw_html.replace(
            "https://unpkg.com/graphiql/graphiql.min.js",
            "https://unpkg.com/graphiql@3/graphiql.min.js",
        );
        let raw_html = raw_html.replace(
            "https://unpkg.com/graphiql/graphiql.min.css",
            "https://unpkg.com/graphiql@3/graphiql.min.css",
        );

        Arc::new(raw_html)
    });

    Html(html.as_str())
}

async fn render_graphql_playground(
    endpoint: &str,
    subscription_endpoint: &str,
) -> impl IntoResponse + Send + Sync {
    _render_graphql_playground(endpoint, subscription_endpoint)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "up": true }))
}

async fn graphql_handler(
    schema: Extension<CoreSchema>,
    req: Json<Request>,
) -> Json<Response> {
    let response = schema.execute(req.0).await;
    let response = unify_response(response);

    response.into()
}

async fn graphql_subscription_handler(
    schema: Extension<CoreSchema>,
    req: Json<Request>,
) -> Sse<impl Stream<Item = anyhow::Result<Event, serde_json::Error>>> {
    let stream = schema.execute_stream(req.0).map(|response| {
        let response = unify_response(response);
        Event::default().json_data(response)
    });
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("keep-alive-text"))
}

async fn ok() -> anyhow::Result<(), ()> {
    Ok(())
}
