use futures::sync::mpsc;
use futures03::stream::SplitStream;
use graphql_parser::parse_query;
use http::StatusCode;
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

use graph::{data::query::QueryTarget, prelude::*};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartPayload {
    query: String,
    variables: Option<serde_json::Value>,
    operation_name: Option<String>,
}

/// GraphQL/WebSocket message received from a client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum IncomingMessage {
    ConnectionInit {
        #[allow(dead_code)]
        payload: Option<serde_json::Value>,
    },
    ConnectionTerminate,
    Start {
        id: String,
        payload: StartPayload,
    },
    Stop {
        id: String,
    },
}

impl IncomingMessage {
    pub fn from_ws_message(msg: WsMessage) -> Result<Self, WsError> {
        let text = msg.into_text()?;
        serde_json::from_str(text.as_str()).map_err(|e| {
            WsError::Http(http::Response::new(Some(format!(
                "Invalid GraphQL over WebSocket message: {}: {}",
                text, e
            ))))
        })
    }
}

/// GraphQL/WebSocket message to be sent to the client.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingMessage {
    ConnectionAck,
    Error {
        id: String,
        payload: String,
    },
    Data {
        id: String,
        payload: Arc<QueryResult>,
    },
    Complete {
        id: String,
    },
}

impl OutgoingMessage {
    pub fn from_query_result(id: String, result: Arc<QueryResult>) -> Self {
        OutgoingMessage::Data {
            id,
            payload: result,
        }
    }

    pub fn from_error_string(id: String, s: String) -> Self {
        OutgoingMessage::Error { id, payload: s }
    }
}

impl From<OutgoingMessage> for WsMessage {
    fn from(msg: OutgoingMessage) -> Self {
        WsMessage::text(serde_json::to_string(&msg).expect("invalid GraphQL/WebSocket message"))
    }
}

/// Helper function to send outgoing messages.
fn send_message(
    sink: &mpsc::UnboundedSender<WsMessage>,
    msg: OutgoingMessage,
) -> Result<(), WsError> {
    sink.unbounded_send(msg.into()).map_err(|_| {
        let mut response = http::Response::new(None);
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        WsError::Http(response)
    })
}

/// Helper function to send error messages.
fn send_error_string(
    sink: &mpsc::UnboundedSender<WsMessage>,
    operation_id: String,
    error: String,
) -> Result<(), WsError> {
    sink.unbounded_send(OutgoingMessage::from_error_string(operation_id, error).into())
        .map_err(|_| {
            let mut response = http::Response::new(None);
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            WsError::Http(response)
        })
}

/// Responsible for recording operation ids and stopping them.
/// On drop, cancels all operations.
struct Operations {
    operations: HashMap<String, CancelGuard>,
    msg_sink: mpsc::UnboundedSender<WsMessage>,
}

impl Operations {
    fn new(msg_sink: mpsc::UnboundedSender<WsMessage>) -> Self {
        Self {
            operations: HashMap::new(),
            msg_sink,
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.operations.contains_key(id)
    }

    fn insert(&mut self, id: String, guard: CancelGuard) {
        self.operations.insert(id, guard);
    }

    fn stop(&mut self, operation_id: String) -> Result<(), WsError> {
        // Remove the operation with this ID from the known operations.
        match self.operations.remove(&operation_id) {
            Some(stopper) => {
                // Cancel the subscription result stream.
                stopper.cancel();

                // Send a GQL_COMPLETE to indicate the operation is been completed.
                send_message(
                    &self.msg_sink,
                    OutgoingMessage::Complete {
                        id: operation_id.clone(),
                    },
                )
            }
            None => send_error_string(
                &self.msg_sink,
                operation_id.clone(),
                format!("Unknown operation ID: {}", operation_id),
            ),
        }
    }
}

impl Drop for Operations {
    fn drop(&mut self) {
        let ids = Vec::from_iter(self.operations.keys().cloned());
        for id in ids {
            // Discard errors, the connection is being shutdown anyways.
            let _ = self.stop(id);
        }
    }
}

/// A WebSocket connection implementing the GraphQL over WebSocket protocol.
pub struct GraphQlConnection<Q, S> {
    id: String,
    logger: Logger,
    graphql_runner: Arc<Q>,
    stream: WebSocketStream<S>,
    deployment: DeploymentHash,
}

impl<Q, S> GraphQlConnection<Q, S>
where
    Q: GraphQlRunner,
    S: AsyncRead + AsyncWrite + Send + 'static + Unpin,
{
    /// Creates a new GraphQL subscription service.
    pub(crate) fn new(
        logger: &Logger,
        deployment: DeploymentHash,
        stream: WebSocketStream<S>,
        graphql_runner: Arc<Q>,
    ) -> Self {
        GraphQlConnection {
            id: Uuid::new_v4().to_string(),
            logger: logger.new(o!("component" => "GraphQlConnection")),
            graphql_runner,
            stream,
            deployment,
        }
    }

    async fn handle_incoming_messages(
        mut ws_stream: SplitStream<WebSocketStream<S>>,
        mut msg_sink: mpsc::UnboundedSender<WsMessage>,
        logger: Logger,
        connection_id: String,
        deployment: DeploymentHash,
        graphql_runner: Arc<Q>,
    ) -> Result<(), WsError> {
        let mut operations = Operations::new(msg_sink.clone());

        // Process incoming messages as long as the WebSocket is open
        while let Some(ws_msg) = ws_stream.try_next().await? {
            use self::IncomingMessage::*;
            use self::OutgoingMessage::*;

            debug!(logger, "Received message";
                   "connection" => &connection_id,
                   "msg" => format!("{}", ws_msg).as_str());

            let msg = IncomingMessage::from_ws_message(ws_msg.clone())?;

            debug!(logger, "GraphQL/WebSocket message";
                   "connection" => &connection_id,
                   "msg" => format!("{:?}", msg).as_str());

            match msg {
                // Always accept connection init requests
                ConnectionInit { payload: _ } => send_message(&msg_sink, ConnectionAck),

                // When receiving a connection termination request
                ConnectionTerminate => {
                    // Close the message sink
                    msg_sink.close().unwrap();

                    // Return an error here to terminate the connection
                    Err(WsError::ConnectionClosed)
                }

                // When receiving a stop request
                Stop { id } => operations.stop(id),

                // When receiving a start request
                Start { id, payload } => {
                    // Respond with a GQL_ERROR if we already have an operation with this ID
                    if operations.contains(&id) {
                        return send_error_string(
                            &msg_sink,
                            id.clone(),
                            format!("Operation with ID already started: {}", id),
                        );
                    }

                    let max_ops = ENV_VARS.graphql.max_operations_per_connection;
                    if operations.operations.len() >= max_ops {
                        return send_error_string(
                            &msg_sink,
                            id,
                            format!("Reached the limit of {} operations per connection", max_ops),
                        );
                    }

                    // Parse the GraphQL query document; respond with a GQL_ERROR if
                    // the query is invalid
                    let query = match parse_query(&payload.query) {
                        Ok(query) => query.into_static(),
                        Err(e) => {
                            return send_error_string(
                                &msg_sink,
                                id,
                                format!("Invalid query: {}: {}", payload.query, e),
                            );
                        }
                    };

                    // Parse the query variables, if present
                    let variables = match payload.variables {
                        None | Some(serde_json::Value::Null) => None,
                        Some(variables @ serde_json::Value::Object(_)) => {
                            match serde_json::from_value(variables.clone()) {
                                Ok(variables) => Some(variables),
                                Err(e) => {
                                    return send_error_string(
                                        &msg_sink,
                                        id,
                                        format!("Invalid variables provided: {}", e),
                                    );
                                }
                            }
                        }
                        _ => {
                            return send_error_string(
                                &msg_sink,
                                id,
                                "Invalid variables provided (must be an object)".to_string(),
                            );
                        }
                    };

                    // Construct a subscription
                    let target = QueryTarget::Deployment(deployment.clone(), Default::default());
                    let subscription = Subscription {
                        // Subscriptions currently do not benefit from the generational cache
                        // anyways, so don't bother passing a network.
                        query: Query::new(query, variables, false),
                    };

                    debug!(logger, "Start operation";
                           "connection" => &connection_id,
                           "id" => &id);

                    // Execute the GraphQL subscription
                    let error_sink = msg_sink.clone();
                    let result_sink = msg_sink.clone();
                    let result_id = id.clone();
                    let err_id = id.clone();
                    let err_connection_id = connection_id.clone();
                    let err_logger = logger.clone();
                    let run_subscription = graphql_runner
                        .cheap_clone()
                        .run_subscription(subscription, target)
                        .compat()
                        .map_err(move |e| {
                            debug!(err_logger, "Subscription error";
                                               "connection" => &err_connection_id,
                                               "id" => &err_id,
                                               "error" => format!("{:?}", e));

                            // Send errors back to the client as GQL_DATA
                            match e {
                                SubscriptionError::GraphQLError(e) => {
                                    // Don't bug clients with transient `TooExpensive` errors,
                                    // simply skip updating them
                                    if !e
                                        .iter()
                                        .any(|err| matches!(err, QueryExecutionError::TooExpensive))
                                    {
                                        let result = Arc::new(QueryResult::from(e));
                                        let msg = OutgoingMessage::from_query_result(
                                            err_id.clone(),
                                            result,
                                        );

                                        // An error means the client closed the websocket, ignore
                                        // and let it be handled in the websocket loop above.
                                        let _ = error_sink.unbounded_send(msg.into());
                                    }
                                }
                            };
                        })
                        .and_then(move |result_stream| {
                            // Send results back to the client as GQL_DATA
                            result_stream
                                .map(move |result| {
                                    OutgoingMessage::from_query_result(result_id.clone(), result)
                                })
                                .map(WsMessage::from)
                                .map(Ok)
                                .compat()
                                .forward(result_sink.sink_map_err(|_| ()))
                                .map(|_| ())
                        });

                    // Setup cancelation.
                    let guard = CancelGuard::new();
                    let logger = logger.clone();
                    let cancel_id = id.clone();
                    let connection_id = connection_id.clone();
                    let run_subscription =
                        run_subscription.compat().cancelable(&guard, move || {
                            debug!(logger, "Stopped operation";
                                       "connection" => &connection_id,
                                       "id" => &cancel_id);
                            Ok(())
                        });
                    operations.insert(id, guard);

                    graph::spawn_allow_panic(run_subscription);
                    Ok(())
                }
            }?
        }
        Ok(())
    }
}

impl<Q, S> IntoFuture for GraphQlConnection<Q, S>
where
    Q: GraphQlRunner,
    S: AsyncRead + AsyncWrite + Send + 'static + Unpin,
{
    type Future = Box<dyn Future<Item = Self::Item, Error = Self::Error> + Send>;
    type Item = ();
    type Error = ();

    fn into_future(self) -> Self::Future {
        debug!(self.logger, "GraphQL over WebSocket connection opened"; "id" => &self.id);

        // Obtain sink/stream pair to send and receive WebSocket messages
        let (ws_sink, ws_stream) = self.stream.split();

        // Allocate a channel for writing
        let (msg_sink, msg_stream) = mpsc::unbounded();

        // Handle incoming messages asynchronously
        let ws_reader = Self::handle_incoming_messages(
            ws_stream,
            msg_sink,
            self.logger.clone(),
            self.id.clone(),
            self.deployment.clone(),
            self.graphql_runner.clone(),
        );

        // Send outgoing messages asynchronously
        let ws_writer = msg_stream.forward(ws_sink.compat().sink_map_err(|_| ()));

        // Silently swallow internal send results and errors. There is nothing
        // we can do about these errors ourselves. Clients will be disconnected
        // as a result of this but most will try to reconnect (GraphiQL for sure,
        // Apollo maybe).
        let ws_writer = ws_writer.map(|_| ());
        let ws_reader = Box::pin(ws_reader.map_err(|_| ()));

        // Return a future that is fulfilled when either we or the client close
        // our/their end of the WebSocket stream
        let logger = self.logger.clone();
        let id = self.id.clone();
        Box::new(ws_reader.compat().select(ws_writer).then(move |_| {
            debug!(logger, "GraphQL over WebSocket connection closed"; "connection" => id);
            Ok(())
        }))
    }
}
