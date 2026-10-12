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
        RawQuery,
    },
    http::{
        HeaderValue,
        StatusCode,
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
use hyper_util::{
    rt::TokioIo,
    server::{
        conn::auto::Builder as ConnectionBuilder,
        graceful::GracefulShutdown,
    },
    service::TowerToHyperService,
};
use serde_json::json;
use std::{
    future::Future,
    net::{
        SocketAddr,
        TcpListener,
    },
    sync::{
        Arc,
        OnceLock,
    },
    time::Duration,
};
use tokio_stream::StreamExt;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::{
    compression::{
        CompressionLayer,
        CompressionLevel,
    },
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub type Service = fuel_core_services::ServiceRunner<GraphqlService>;

pub use super::database::ReadDatabase;
use super::ports::{
    DatabaseDaCompressedBlocks,
    OnChainDatabaseAt,
    worker,
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
    server: tokio::task::JoinHandle<anyhow::Result<()>>,
    /// Handle to the inner `AsyncProcessor`'s task tracker. Kept here so
    /// `Task::shutdown` can `await processor.drain()` and wait for every
    /// hyper request future to complete before the surrounding service
    /// teardown reaches `AsyncProcessor::drop`. If we skip this, the
    /// `Runtime::shutdown_timeout` in `Drop` would detach any task still
    /// running at its deadline; the detached worker then outlives the
    /// service and races rocksdb's global `Env::Default()` destructor in
    /// `__run_exit_handlers`, surfacing as SIGABRT/SIGSEGV at process
    /// exit.
    processor: Arc<AsyncProcessor>,
    force_shutdown: tokio::sync::watch::Sender<bool>,
}

const GRAPHQL_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);
const GRAPHQL_ACCEPT_ERROR_RETRY_DELAY: Duration = Duration::from_secs(1);

async fn accept_with_retry<T, A, F>(mut accept: A, retry_delay: Duration) -> T
where
    A: FnMut() -> F,
    F: Future<Output = std::io::Result<T>>,
{
    loop {
        match accept().await {
            Ok(connection) => return connection,
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                tracing::debug!("Accepted GraphQL connection already errored: {err}");
            }
            Err(err) => {
                tracing::error!(
                    retry_delay_secs = retry_delay.as_secs(),
                    "GraphQL accept error: {err}; retrying"
                );
                tokio::time::sleep(retry_delay).await;
            }
        }
    }
}

#[derive(Clone)]
struct ExecutorWithMetrics {
    processor: Arc<AsyncProcessor>,
    /// Unlike normal service shutdown, this signal is sent only after the
    /// graceful connection-drain deadline expires. Until then, Hyper request
    /// futures remain alive so in-flight responses can finish.
    force_shutdown: tokio::sync::watch::Receiver<bool>,
}

impl<F> Executor<F> for ExecutorWithMetrics
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        let mut force_shutdown = self.force_shutdown.clone();
        let wrapped = async move {
            tokio::select! {
                _ = fut => {}
                _ = force_shutdown.wait_for(|shutdown| *shutdown) => {}
            }
        };
        let result = self.processor.try_spawn(wrapped);

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

        let processor = Arc::new(AsyncProcessor::new(
            "GraphQLFutures",
            number_of_threads,
            tokio::sync::Semaphore::MAX_PERMITS,
        )?);
        let (force_shutdown, force_shutdown_receiver) =
            tokio::sync::watch::channel(false);
        let executor = ExecutorWithMetrics {
            processor: processor.clone(),
            force_shutdown: force_shutdown_receiver,
        };
        listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(listener)?;
        let server = async move {
            let graceful = GracefulShutdown::new();
            loop {
                tokio::select! {
                    connection = accept_with_retry(
                        || listener.accept(),
                        GRAPHQL_ACCEPT_ERROR_RETRY_DELAY,
                    ) => {
                        let (stream, _) = connection;
                        let io = TokioIo::new(stream);
                        let service = TowerToHyperService::new(router.clone());
                        let connection_executor = executor.clone();
                        let graceful_watcher = graceful.watcher();
                        executor.execute(async move {
                            let builder = ConnectionBuilder::new(connection_executor);
                            let connection =
                                builder.serve_connection_with_upgrades(io, service);
                            if let Err(err) = graceful_watcher.watch(connection).await {
                                tracing::debug!(
                                    "GraphQL connection closed with error: {err}"
                                );
                            }
                        });
                    }
                    _ = state.wait_stopping_or_stopped() => {
                        graceful.shutdown().await;
                        return Ok(())
                    },
                }
            }
        };

        Ok(Task {
            server: tokio::spawn(server),
            processor,
            force_shutdown,
        })
    }
}

impl RunnableTask for Task {
    async fn run(&mut self, state: &mut StateWatcher) -> TaskNextAction {
        // Allow the `StateWatcher` to override the "graceful shutdown" of the internal GraphQL
        // server. If the service is taking too long to shutdown, we abort the server task.
        tokio::select! {
            result = &mut self.server => map_server_result(result),
            result = state.while_started() => {
                match result {
                    Ok(_) => {
                        match tokio::time::timeout(
                            GRAPHQL_SHUTDOWN_TIMEOUT,
                            &mut self.server,
                        ).await {
                            Ok(result) => map_server_result(result),
                            Err(_) => {
                                tracing::warn!(
                                    timeout_secs = GRAPHQL_SHUTDOWN_TIMEOUT.as_secs(),
                                    "GraphQL shutdown timed out; aborting server task"
                                );
                                let _ = self.force_shutdown.send(true);
                                self.server.abort();
                                map_server_result((&mut self.server).await)
                            }
                        }
                    }
                    Err(err) => TaskNextAction::ErrorContinue(err),
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        let _ = self.force_shutdown.send(true);
        // The GraphQL accept loop has already stopped accepting new requests,
        // but some hyper request futures may still be running on
        // the inner `AsyncProcessor` runtime — connection-drain tasks and
        // any handler whose response was mid-flight when the stop signal
        // arrived. We must wait for them to complete here, because
        // `AsyncProcessor::drop` falls back to `Runtime::shutdown_timeout`
        // which detaches any task still running at its deadline; the
        // detached worker then outlives the service and races rocksdb's
        // global `Env::Default()` destructor in `__run_exit_handlers`,
        // surfacing as SIGABRT/SIGSEGV at process exit.
        self.processor.drain().await;
        Ok(())
    }
}

fn map_server_result(
    result: Result<anyhow::Result<()>, tokio::task::JoinError>,
) -> TaskNextAction {
    match result {
        Ok(Ok(())) => {
            // The accept loop only returns `Ok` after observing service shutdown.
            TaskNextAction::Stop
        }
        Ok(Err(err)) => {
            tracing::error!("GraphQL server task exited with error: {err}");
            TaskNextAction::Stop
        }
        Err(err) if err.is_cancelled() => TaskNextAction::Stop,
        Err(err) => {
            tracing::error!("GraphQL server task join failed: {err}");
            TaskNextAction::Stop
        }
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
    readiness: crate::service::adapters::ready_signal::Readiness,
    p2p_service: P2pService,
    gas_price_provider: GasPriceProvider,
    chain_state_info_provider: ChainInfoProvider,
    memory_pool: SharedMemoryPool,
    worker_shared_state: graphql_api::worker_service::SharedState,
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
        .extension(ChainStateInfoExtension::new(worker_shared_state.block_height_subscription_handler.subscribe()))
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
        .data(worker_shared_state.clone())
        .extension(ValidationExtension::new(
            max_queries_resolver_recursive_depth,
        ))
        .extension(async_graphql::extensions::Tracing)
        .extension(RequiredFuelBlockHeightExtension::new(
            required_fuel_block_height_tolerance,
            required_fuel_block_height_timeout,
            worker_shared_state.block_height_subscription_handler.subscribe(),
        ))
        .finish();

    let graphql_endpoint = "/v1/graphql";
    let graphql_subscription_endpoint = "/v1/graphql-sub";

    let graphql_playground = |query: RawQuery| {
        render_graphql_playground(graphql_endpoint, graphql_subscription_endpoint, query)
    };

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
        .route("/v1/ready", get(ready))
        .layer(Extension(readiness))
        .layer(Extension(schema))
        // The default predicate skips `text/event-stream`, so SSE subscriptions
        // are streamed without compression.
        .layer(CompressionLayer::new().quality(CompressionLevel::Fastest))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
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
/// This is because the rendering and replacing is expensive.
/// Auth / provider query params are applied at request time via a cheap
/// string replace so the expensive template build stays cached.
static GRAPHQL_PLAYGROUND_HTML: OnceLock<Arc<String>> = OnceLock::new();

const PLAYGROUND_ENDPOINT_PLACEHOLDER: &str = "__FUEL_GRAPHQL_ENDPOINT__";
const PLAYGROUND_SUBSCRIPTION_PLACEHOLDER: &str = "__FUEL_GRAPHQL_SUBSCRIPTION_ENDPOINT__";
const PLAYGROUND_AUTH_PLACEHOLDER: &str = "__FUEL_GRAPHQL_AUTH__";

/// Escape a value for safe embedding inside a single-quoted JavaScript string
/// in the GraphiQL HTML page (prevents query/token breakout / XSS).
fn js_string_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            _ => out.push(c),
        }
    }
    out
}

fn _render_graphql_playground(
    endpoint: &str,
    subscription_endpoint: &str,
    query: RawQuery,
) -> impl IntoResponse + Send + Sync {
    let html = GRAPHQL_PLAYGROUND_HTML.get_or_init(|| {
        // Include Authorization in the template so GraphiQL emits a `headers`
        // object we can rewrite per request (`.header()` is required for that).
        let raw_html = GraphiQLSource::build()
            .endpoint(PLAYGROUND_ENDPOINT_PLACEHOLDER)
            .subscription_endpoint(PLAYGROUND_SUBSCRIPTION_PLACEHOLDER)
            .header("Authorization", PLAYGROUND_AUTH_PLACEHOLDER)
            .header("Content-Type", "application/json")
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

    // Forward playground URL query (e.g. ?token=... used by QuickNode/Ankr)
    // onto the GraphQL HTTP and subscription endpoints, and surface common
    // auth params as an Authorization header GraphiQL will send.
    let qs = query
        .0
        .as_deref()
        .filter(|q| !q.is_empty())
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let endpoint_js = js_string_escape(&format!("{endpoint}{qs}"));
    let subscription_js = js_string_escape(&format!("{subscription_endpoint}{qs}"));

    let mut page = html
        .replace(PLAYGROUND_ENDPOINT_PLACEHOLDER, &endpoint_js)
        .replace(PLAYGROUND_SUBSCRIPTION_PLACEHOLDER, &subscription_js);

    if let Some(auth_header) = playground_auth_header(query.0.as_deref()) {
        page = page.replace(PLAYGROUND_AUTH_PLACEHOLDER, &js_string_escape(&auth_header));
    } else {
        // Drop the unused Authorization entry when no token was supplied.
        page = page
            .replace("'Authorization': '__FUEL_GRAPHQL_AUTH__',", "")
            .replace("\"Authorization\": \"__FUEL_GRAPHQL_AUTH__\",", "")
            .replace(PLAYGROUND_AUTH_PLACEHOLDER, "");
    }

    Html(page)
}

/// Map common provider query keys to an HTTP Authorization header value.
fn playground_auth_header(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?.to_ascii_lowercase();
        let value = parts.next().unwrap_or("");
        let value = urlencoding_decode(value);
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "authorization" => return Some(value),
            "token" | "api_key" | "apikey" | "access_token" => {
                if value.to_ascii_lowercase().starts_with("bearer ") {
                    return Some(value);
                }
                return Some(format!("Bearer {value}"));
            }
            _ => {}
        }
    }
    None
}

fn urlencoding_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = from_hex(bytes[i + 1]);
                let h2 = from_hex(bytes[i + 2]);
                if let (Some(a), Some(b)) = (h1, h2) {
                    out.push((a << 4) | b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

async fn render_graphql_playground(
    endpoint: &str,
    subscription_endpoint: &str,
    query: RawQuery,
) -> impl IntoResponse + Send + Sync {
    _render_graphql_playground(endpoint, subscription_endpoint, query)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "up": true }))
}

/// `/v1/ready` — distinct from `/v1/health` (liveness, always dumb-up).
/// `ready = services_started && (poa disabled || poa.is_ready())`.
/// Leader-lock status is deliberately excluded: readiness is not leadership.
async fn ready(
    Extension(r): Extension<crate::service::adapters::ready_signal::Readiness>,
) -> (StatusCode, Json<serde_json::Value>) {
    let services_started = r.services_started();
    let poa_enabled = r.poa_enabled();
    let synced = r.poa_ready().await;
    let ready = services_started && synced;
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "ready": ready,
            "services_started": services_started,
            "poa_enabled": poa_enabled,
            "synced": synced,
        })),
    )
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
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    use fuel_core_services::Service as _;
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    #[test]
    fn js_string_escape_neutralizes_quote_breakout() {
        assert_eq!(
            js_string_escape("Bearer x';alert(1)//"),
            "Bearer x\\';alert(1)//"
        );
        assert_eq!(js_string_escape("a<b>&c"), "a\\u003cb\\u003e\\u0026c");
    }

    #[tokio::test]
    async fn accept_retries_after_resource_error() {
        let mut results = std::collections::VecDeque::from([
            Err(std::io::Error::other("too many open files")),
            Ok(42),
        ]);

        let connection = accept_with_retry(
            || {
                std::future::ready(
                    results.pop_front().expect("unexpected accept attempt"),
                )
            },
            Duration::ZERO,
        )
        .await;

        assert_eq!(connection, 42);
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn shutdown_gracefully_finishes_in_flight_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bound_address = listener.local_addr().unwrap();
        let request_started = Arc::new(tokio::sync::Notify::new());
        let release_request = Arc::new(tokio::sync::Notify::new());
        let router = Router::new().route(
            "/slow",
            get({
                let request_started = request_started.clone();
                let release_request = release_request.clone();
                move || {
                    let request_started = request_started.clone();
                    let release_request = release_request.clone();
                    async move {
                        request_started.notify_one();
                        release_request.notified().await;
                        tokio::task::yield_now().await;
                        "done"
                    }
                }
            }),
        );
        let service = fuel_core_services::ServiceRunner::new_with_params(
            GraphqlService { bound_address },
            ServerParams {
                router,
                listener,
                number_of_threads: 1,
            },
        );
        assert!(service.start_and_await().await.unwrap().started());

        let response = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(bound_address).await.unwrap();
            stream
                .write_all(
                    b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.unwrap();
            response
        });
        request_started.notified().await;
        let shutdown_started = tokio::time::Instant::now();
        assert!(service.stop());
        assert!(service.state().stopping());
        release_request.notify_one();

        let (stopped, response) = tokio::join!(service.await_stop(), response);
        assert!(
            shutdown_started.elapsed() < GRAPHQL_SHUTDOWN_TIMEOUT,
            "in-flight request hit the forced shutdown deadline"
        );

        assert!(stopped.unwrap().stopped());
        let response = String::from_utf8(response.unwrap()).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("done"));
    }
}
