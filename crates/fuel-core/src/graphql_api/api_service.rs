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
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::p2p::PeerInfo,
};
use futures::Stream;
use hyper::rt::Executor;
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
        atomic::{
            AtomicBool,
            Ordering,
        },
    },
    time::Duration,
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
struct Readiness {
    block_production_ready_signal: crate::service::adapters::ready_signal::ReadySignal,
    poa: crate::service::adapters::PoAAdapter,
    p2p_service: Arc<P2pService>,
    read_database: Arc<ReadDatabase>,
    /// Latches once either sync signal (PoA's own quiet-window `SyncState`, or this node's
    /// height catching up to its peers') is first observed satisfied. See `ready()` for why
    /// two independent signals feed the same latch. Once caught up once, treat sync as
    /// satisfied rather than flapping readiness with it.
    has_synced_once: Arc<AtomicBool>,
}

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
    server: tokio::task::JoinHandle<hyper::Result<()>>,
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
}

const GRAPHQL_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone)]
struct ExecutorWithMetrics {
    processor: Arc<AsyncProcessor>,
    /// Watched alongside every spawned request future so that the future
    /// resolves as soon as the service leaves `Started`. This bounds the
    /// time `Task::shutdown` has to wait inside `processor.drain()`: even
    /// a handler stuck in a long-running await terminates at the stop
    /// signal instead of being detached at the end of
    /// `Runtime::shutdown_timeout` and racing the rocksdb static
    /// destructors at process atexit.
    state: StateWatcher,
}

impl<F> Executor<F> for ExecutorWithMetrics
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        let mut state = self.state.clone();
        let wrapped = async move {
            // We don't need the future's output (axum/hyper drives the
            // request lifecycle via I/O on the connection itself), so
            // dropping it on shutdown is safe — the connection's drop
            // closes the socket cleanly.
            tokio::select! {
                _ = fut => {}
                _ = state.while_started() => {}
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

        let executor = ExecutorWithMetrics {
            processor: processor.clone(),
            state: state.clone(),
        };

        let server = axum::Server::from_tcp(listener)
            .unwrap()
            .executor(executor)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                // Wait for an actual stop signal. Using `while_started` here would
                // resolve immediately while the state is still `Starting`, racing
                // with the runner setting the state to `Started`. When the race
                // is lost, the spawned server task aborts before it ever serves
                // a request — which then cascades into a full FuelService
                // shutdown via `select_all(await_stop)` in `service.rs::run`.
                //
                // A `wait_stopping_or_stopped` error means the watcher's sender
                // was already dropped (the `ServiceRunner` is being torn down).
                // That is *also* a stop signal — we want graceful shutdown to
                // fire — so swallow the error rather than panicking, which would
                // crash the spawned server task and leak the listener.
                let _ = state.wait_stopping_or_stopped().await;
            });

        Ok(Task {
            server: tokio::spawn(server),
            processor,
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
        // The `axum::Server` has already gracefully stopped accepting new
        // requests, but some hyper request futures may still be running on
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
    result: Result<hyper::Result<()>, tokio::task::JoinError>,
) -> TaskNextAction {
    match result {
        Ok(Ok(())) => {
            // The `axum::Server` has its internal loop. If `await` is finished, we get an internal
            // error or stop signal.
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
    poa_adapter: crate::service::adapters::PoAAdapter,
    block_production_ready_signal: crate::service::adapters::ready_signal::ReadySignal,
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
    let combined_read_database = Arc::new(ReadDatabase::new(
        config.config.database_batch_size,
        genesis_block_height,
        on_database,
        off_database,
    )?);
    let p2p_service: Arc<P2pService> = Arc::new(p2p_service);
    // Split off before `.data(...)` moves the originals into the GraphQL schema context.
    let readiness_read_database = combined_read_database.clone();
    let readiness_p2p_service = p2p_service.clone();
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

    let graphql_playground =
        || render_graphql_playground(graphql_endpoint, graphql_subscription_endpoint);

    let readiness = Readiness {
        block_production_ready_signal,
        poa: poa_adapter,
        p2p_service: readiness_p2p_service,
        read_database: readiness_read_database,
        has_synced_once: Arc::new(AtomicBool::new(false)),
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

/// Computes the latched `synced` bit for `/v1/ready`. `None` (PoA disabled) is trivially
/// synced. `Synced` latches `has_synced_once` permanently true. `NotSynced` reports the
/// latch as-is, so a transient network-block-triggered flap doesn't flip readiness back
/// off once the node has genuinely caught up at least once.
fn latch_synced(
    sync_state: Option<&fuel_core_poa::sync::SyncState>,
    has_synced_once: &AtomicBool,
) -> bool {
    use fuel_core_poa::sync::SyncState;

    match sync_state {
        None => true,
        Some(SyncState::Synced(_)) => {
            has_synced_once.store(true, Ordering::Release);
            true
        }
        Some(SyncState::NotSynced) => has_synced_once.load(Ordering::Acquire),
    }
}

/// Pure comparison: is `local_height` at or above the highest height any peer has
/// reported via heartbeat? `None` means no peer has reported a height yet, so this signal
/// can't confirm anything either way (caller should not treat that as "not synced").
fn is_height_caught_up_to_peers(
    local_height: BlockHeight,
    peer_heights: &[BlockHeight],
) -> Option<bool> {
    peer_heights
        .iter()
        .max()
        .map(|&max_peer_height| local_height >= max_peer_height)
}

/// Independent alternative to PoA's quiet-window `SyncState`: this node's own imported
/// height compared against the highest height its connected peers have reported via
/// heartbeat. Exists because `SyncState`'s `NotSynced -> Synced` transition requires a
/// quiet window (`--time-until-synced`) with no new blocks, including locally-produced
/// ones — on any chain producing blocks faster than that window (e.g. `--poa-open-period`
/// shorter than `--time-until-synced`), that transition can structurally never fire, so a
/// genuinely caught-up node would otherwise never pass `/v1/ready` (DEVOPS-1518).
async fn height_caught_up_to_peers(
    p2p_service: &P2pService,
    read_database: &ReadDatabase,
) -> bool {
    let Ok(local_height) = read_database.view().and_then(|view| view.latest_height())
    else {
        return false;
    };
    let Ok(peers) = p2p_service.all_peer_info().await else {
        return false;
    };
    let peer_heights: Vec<BlockHeight> = peers
        .iter()
        .filter_map(|peer: &PeerInfo| peer.heartbeat_data.block_height)
        .collect();
    is_height_caught_up_to_peers(local_height, &peer_heights).unwrap_or(false)
}

/// `/v1/ready` — distinct from `/v1/health` (liveness, always dumb-up). Reports whether
/// this node is DB-open (all sub-services finished startup) and, when PoA is enabled,
/// p2p-synced. Leader-lock status is deliberately excluded: readiness is not leadership,
/// and gating on it would knock healthy followers out of Service Endpoints.
async fn ready(
    Extension(r): Extension<Readiness>,
) -> (StatusCode, Json<serde_json::Value>) {
    let services_started = r.block_production_ready_signal.is_ready();
    let sync_state = r.poa.sync_state();
    let mut synced = latch_synced(sync_state.as_ref(), &r.has_synced_once);
    if !synced
        && sync_state.is_some()
        && height_caught_up_to_peers(&r.p2p_service, &r.read_database).await
    {
        r.has_synced_once.store(true, Ordering::Release);
        synced = true;
    }
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
            "poa_enabled": sync_state.is_some(),
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

#[cfg(test)]
#[allow(non_snake_case)]
mod readiness_tests {
    use super::*;
    use fuel_core_poa::sync::SyncState;
    use fuel_core_types::{
        blockchain::header::BlockHeader,
        fuel_types::BlockHeight,
        tai64::Tai64,
    };

    #[test]
    fn latch_synced__none_is_trivially_synced() {
        let latch = AtomicBool::new(false);

        assert!(latch_synced(None, &latch));
    }

    #[test]
    fn latch_synced__synced_sets_the_latch() {
        let latch = AtomicBool::new(false);
        let header = BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now());
        let state = SyncState::Synced(Arc::new(header));

        assert!(latch_synced(Some(&state), &latch));
        assert!(latch.load(Ordering::Acquire));
    }

    #[test]
    fn latch_synced__not_synced_before_latch_reports_not_synced() {
        let latch = AtomicBool::new(false);

        assert!(!latch_synced(Some(&SyncState::NotSynced), &latch));
    }

    #[test]
    fn latch_synced__not_synced_after_latch_stays_synced() {
        let latch = AtomicBool::new(true);

        assert!(latch_synced(Some(&SyncState::NotSynced), &latch));
    }

    #[test]
    fn is_height_caught_up_to_peers__no_peers_is_unknown() {
        let local_height = BlockHeight::from(10u32);

        assert_eq!(is_height_caught_up_to_peers(local_height, &[]), None);
    }

    #[test]
    fn is_height_caught_up_to_peers__local_at_or_above_max_peer_is_caught_up() {
        let local_height = BlockHeight::from(10u32);
        let peer_heights = [BlockHeight::from(8u32), BlockHeight::from(10u32)];

        assert_eq!(
            is_height_caught_up_to_peers(local_height, &peer_heights),
            Some(true)
        );
    }

    #[test]
    fn is_height_caught_up_to_peers__local_below_max_peer_is_not_caught_up() {
        let local_height = BlockHeight::from(10u32);
        let peer_heights = [BlockHeight::from(8u32), BlockHeight::from(11u32)];

        assert_eq!(
            is_height_caught_up_to_peers(local_height, &peer_heights),
            Some(false)
        );
    }
}
