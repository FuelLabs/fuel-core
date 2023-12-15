use graph::{
    data::query::QueryTarget,
    prelude::{SubscriptionServer as SubscriptionServerTrait, *},
};
use http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE};
use http::{HeaderValue, Response, StatusCode};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::Request;

use crate::connection::GraphQlConnection;

/// A GraphQL subscription server based on Hyper / Websockets.
pub struct SubscriptionServer<Q, S> {
    logger: Logger,
    graphql_runner: Arc<Q>,
    store: Arc<S>,
}

impl<Q, S> SubscriptionServer<Q, S>
where
    Q: GraphQlRunner,
    S: QueryStoreManager,
{
    pub fn new(logger: &Logger, graphql_runner: Arc<Q>, store: Arc<S>) -> Self {
        SubscriptionServer {
            logger: logger.new(o!("component" => "SubscriptionServer")),
            graphql_runner,
            store,
        }
    }

    async fn subgraph_id_from_url_path(
        store: Arc<S>,
        path: &str,
    ) -> Result<Option<DeploymentState>, Error> {
        fn target_from_name(name: String, api_version: ApiVersion) -> Option<QueryTarget> {
            SubgraphName::new(name)
                .ok()
                .map(|sub_name| QueryTarget::Name(sub_name, api_version))
        }

        fn target_from_id(id: &str, api_version: ApiVersion) -> Option<QueryTarget> {
            DeploymentHash::new(id)
                .ok()
                .map(|hash| QueryTarget::Deployment(hash, api_version))
        }

        async fn state<S: QueryStoreManager>(
            store: Arc<S>,
            target: Option<QueryTarget>,
        ) -> Option<DeploymentState> {
            let target = match target {
                Some(target) => target,
                None => return None,
            };
            match store.query_store(target, false).await.ok() {
                Some(query_store) => query_store.deployment_state().await.ok(),
                None => None,
            }
        }

        let path_segments = {
            let mut segments = path.split('/');

            // Remove leading '/'
            let first_segment = segments.next();
            if first_segment != Some("") {
                return Ok(None);
            }

            segments.collect::<Vec<_>>()
        };

        match path_segments.as_slice() {
            &["subgraphs", "id", subgraph_id] => {
                Ok(state(store, target_from_id(subgraph_id, ApiVersion::default())).await)
            }
            &["subgraphs", "name", _] | &["subgraphs", "name", _, _] => Ok(state(
                store,
                target_from_name(path_segments[2..].join("/"), ApiVersion::default()), // TODO: version
            )
            .await),
            &["subgraphs", "network", _, _] => Ok(state(
                store,
                target_from_name(path_segments[1..].join("/"), ApiVersion::default()), // TODO: version
            )
            .await),
            _ => Ok(None),
        }
    }
}

#[async_trait]
impl<Q, S> SubscriptionServerTrait for SubscriptionServer<Q, S>
where
    Q: GraphQlRunner,
    S: QueryStoreManager,
{
    async fn serve(self, port: u16) {
        info!(
            self.logger,
            "Starting GraphQL WebSocket server at: ws://localhost:{}", port
        );

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
        let socket = TcpListener::bind(&addr)
            .await
            .expect("Failed to bind WebSocket port");

        loop {
            let stream = match socket.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    trace!(self.logger, "Connection error: {}", e);
                    continue;
                }
            };
            let logger = self.logger.clone();
            let logger2 = self.logger.clone();
            let graphql_runner = self.graphql_runner.clone();
            let store = self.store.clone();

            // Subgraph that the request is resolved to (if any)
            let subgraph_id = Arc::new(Mutex::new(None));
            let accept_subgraph_id = subgraph_id.clone();

            accept_hdr_async(stream, move |request: &Request, mut response: Response<()>| {
                // Try to obtain the subgraph ID or name from the URL path.
                // Return a 404 if the URL path contains no name/ID segment.
                let path = request.uri().path();

                // `block_in_place` is not recommended but in this case we have no alternative since
                // we're in an async context but `tokio_tungstenite` doesn't allow this callback
                // to be a future.
                let state = tokio::task::block_in_place(|| {
                    graph::block_on(Self::subgraph_id_from_url_path(
                        store.clone(),
                        path,
                    ))
                })
                .map_err(|e| {
                    error!(
                        logger,
                        "Error resolving subgraph ID from URL path";
                        "error" => e.to_string()
                    );

                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .header(CONTENT_TYPE, "text/plain")
                        .body(None)
                        .unwrap()
                })
                .and_then(|state| {
                    state.ok_or_else(|| {
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .header(CONTENT_TYPE, "text/plain")
                            .body(None)
                            .unwrap()
                    })
                })?;

                // Check if the subgraph is deployed
                if !state.is_deployed() {
                        error!(logger, "Failed to establish WS connection, no data found for subgraph";
                                        "subgraph_id" => state.id.to_string(),
                        );
                        return Err(Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .header(CONTENT_TYPE, "text/plain")
                            .body(None)
                            .unwrap());
                    }

                *accept_subgraph_id.lock().unwrap() = Some(state.id);
                response.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    HeaderValue::from_static("graphql-ws"),
                );
                Ok(response)
            })
            .then(move |result| async move {
                match result {
                    Ok(ws_stream) => {
                        // Obtain the subgraph ID or name that we resolved the request to
                        let subgraph_id = subgraph_id.lock().unwrap().clone().unwrap();

                        // Spawn a GraphQL over WebSocket connection
                        let service = GraphQlConnection::new(
                            &logger2,
                            subgraph_id,
                            ws_stream,
                            graphql_runner.clone(),
                        );

                        graph::spawn_allow_panic(service.into_future().compat());
                    }
                    Err(e) => {
                        // We gracefully skip over failed connection attempts rather
                        // than tearing down the entire stream
                        trace!(logger2, "Failed to establish WebSocket connection: {}", e);
                    }
                }
            }).await
        }
    }
}
