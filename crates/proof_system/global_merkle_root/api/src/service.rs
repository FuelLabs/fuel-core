use std::{
    future::Future,
    net::{
        SocketAddr,
        TcpListener,
        ToSocketAddrs,
    },
    pin::Pin,
};

use async_graphql::{
    http::GraphiQLSource,
    EmptyMutation,
    EmptySubscription,
    Request,
    Response,
};
use axum::{
    response::{
        Html,
        IntoResponse,
    },
    routing,
    Extension,
    Json,
    Router,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};

use crate::{
    ports::GetStateRoot,
    schema::{
        Query,
        Schema,
    },
};

/// Create a new state root service within a service runner.
pub fn new_service<Storage, Addr>(
    storage: Storage,
    network_address: Addr,
) -> anyhow::Result<ServiceRunner<StateRootApiService>>
where
    Storage: GetStateRoot + Send + Sync + 'static,
    Addr: ToSocketAddrs,
{
    Ok(ServiceRunner::new(StateRootApiService::new(
        storage,
        network_address,
    )?))
}

/// The state root API service.
pub struct StateRootApiService {
    router: Router<hyper::Body>,
    listener: TcpListener,
    bound_address: SocketAddr,
}

/// The shared state of the state root service.
/// Primarily used to expose the bound address
/// in tests.
#[derive(Clone, Debug)]
pub struct SharedState {
    /// The address the service listens to requests on.
    pub bound_address: SocketAddr,
}

impl StateRootApiService {
    #[tracing::instrument(skip(storage, network_address))]
    fn new<Storage, Addr>(storage: Storage, network_address: Addr) -> anyhow::Result<Self>
    where
        Storage: GetStateRoot + Send + Sync + 'static,
        Addr: ToSocketAddrs,
    {
        let graphql_endpoint = "/graphql";

        let graphql_playground = || render_graphql_playground(graphql_endpoint);

        let query = Query::new(storage);
        let schema = Schema::build(query, EmptyMutation, EmptySubscription).finish();

        let router = Router::<hyper::Body>::new()
            .route("/playground", routing::get(graphql_playground))
            .route(graphql_endpoint, routing::post(graphql_handler::<Storage>))
            .layer(Extension(schema));

        let listener = TcpListener::bind(network_address)?;
        let bound_address = listener.local_addr()?;

        Ok(Self {
            router,
            listener,
            bound_address,
        })
    }
}

async fn render_graphql_playground(graphql_endpoint: &str) -> impl IntoResponse {
    Html(
        GraphiQLSource::build()
            .endpoint(graphql_endpoint)
            .title("Fuel Global Merkle Root Service Graphql Playground")
            .finish(),
    )
}

async fn graphql_handler<Storage>(
    schema: Extension<Schema<Storage>>,
    request: Json<Request>,
) -> Json<Response>
where
    Storage: GetStateRoot + Send + Sync + 'static,
{
    schema.execute(request.0).await.into()
}

#[async_trait::async_trait]
impl RunnableService for StateRootApiService {
    const NAME: &'static str = "StateRootGraphQL";

    type SharedData = SharedState;
    type Task = StateRootApiTask;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        SharedState {
            bound_address: self.bound_address,
        }
    }

    #[tracing::instrument(skip(self, state_watcher, _params))]
    async fn into_task(
        self,
        state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let mut state = state_watcher.clone();

        let graceful_shutdown_signal = async move {
            state.while_started().await.expect("unexpected termination");
        };

        tracing::info!(%self.bound_address, "listening for GraphQL requests");

        let server = Box::pin(
            axum::Server::from_tcp(self.listener)?
                .serve(self.router.into_make_service())
                .with_graceful_shutdown(graceful_shutdown_signal),
        );

        Ok(StateRootApiTask { server })
    }
}

/// The runnable task for the state root service.
/// The task is holding the server future which is
/// awaited when the task is being run.
///
/// We use dynamic dispatch/type erasure because the concrete
/// type unfortunately isn't exposed by Axum.
pub struct StateRootApiTask {
    server: Pin<Box<dyn Future<Output = hyper::Result<()>> + Send + 'static>>,
}

impl RunnableTask for StateRootApiTask {
    async fn run(
        &mut self,
        state_watcher: &mut fuel_core_services::StateWatcher,
    ) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = state_watcher.while_started() => {
                tracing::debug!("stopping state root API service");
                TaskNextAction::Stop
            },

            action = self.serve() => action,
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl StateRootApiTask {
    async fn serve(&mut self) -> TaskNextAction {
        match self.server.as_mut().await {
            Ok(()) => {
                // The `axum::Server` has stopped, and so should we
                TaskNextAction::Stop
            }
            Err(error) => {
                tracing::error!(%error, "state root axum server returned error");
                TaskNextAction::Stop
            }
        }
    }
}
