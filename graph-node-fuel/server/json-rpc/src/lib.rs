use graph::prelude::{Value as GraphValue, *};
use jsonrpsee::core::Error as JsonRpcError;
use jsonrpsee::http_server::{HttpServerBuilder, HttpServerHandle};
use jsonrpsee::types::error::CallError;
use jsonrpsee::types::ErrorObject;
use jsonrpsee::RpcModule;
use serde_json::{self, Value as JsonValue};

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};

type JsonRpcResult<T> = Result<T, jsonrpsee::core::Error>;

pub struct JsonRpcServer {
    // TODO: in the future we might want to have some sort of async drop to stop
    // the server. For now, we're just letting it run it forever.
    _handle: HttpServerHandle,
}

impl JsonRpcServer {
    pub async fn serve<R>(
        port: u16,
        http_port: u16,
        ws_port: u16,
        registrar: Arc<R>,
        node_id: NodeId,
        logger: Logger,
    ) -> JsonRpcResult<Self>
    where
        R: SubgraphRegistrar,
    {
        let logger = logger.new(o!("component" => "JsonRpcServer"));

        info!(
            logger,
            "Starting JSON-RPC admin server at: http://localhost:{}", port
        );

        let state = ServerState {
            registrar,
            http_port,
            ws_port,
            node_id,
            logger,
        };

        let socket_addr: SocketAddr = (Ipv4Addr::new(0, 0, 0, 0), port).into();
        let http_server = HttpServerBuilder::default().build(socket_addr).await?;

        let mut rpc_module = RpcModule::new(state);
        rpc_module
            .register_async_method("subgraph_create", |params, state| async move {
                state.create_handler(params.parse()?).await
            })
            .unwrap();
        rpc_module
            .register_async_method("subgraph_deploy", |params, state| async move {
                state.deploy_handler(params.parse()?).await
            })
            .unwrap();
        rpc_module
            .register_async_method("subgraph_remove", |params, state| async move {
                state.remove_handler(params.parse()?).await
            })
            .unwrap();
        rpc_module
            .register_async_method("subgraph_reassign", |params, state| async move {
                state.reassign_handler(params.parse()?).await
            })
            .unwrap();

        let _handle = http_server.start(rpc_module)?;
        Ok(Self { _handle })
    }
}

struct ServerState<R> {
    registrar: Arc<R>,
    http_port: u16,
    ws_port: u16,
    node_id: NodeId,
    logger: Logger,
}

impl<R: SubgraphRegistrar> ServerState<R> {
    const DEPLOY_ERROR: i64 = 0;
    const REMOVE_ERROR: i64 = 1;
    const CREATE_ERROR: i64 = 2;
    const REASSIGN_ERROR: i64 = 3;

    /// Handler for the `subgraph_create` endpoint.
    async fn create_handler(&self, params: SubgraphCreateParams) -> JsonRpcResult<JsonValue> {
        info!(&self.logger, "Received subgraph_create request"; "params" => format!("{:?}", params));

        match self.registrar.create_subgraph(params.name.clone()).await {
            Ok(result) => {
                Ok(serde_json::to_value(result).expect("invalid subgraph creation result"))
            }
            Err(e) => Err(json_rpc_error(
                &self.logger,
                "subgraph_create",
                e,
                Self::CREATE_ERROR,
                params,
            )),
        }
    }

    /// Handler for the `subgraph_deploy` endpoint.
    async fn deploy_handler(&self, params: SubgraphDeployParams) -> JsonRpcResult<JsonValue> {
        info!(&self.logger, "Received subgraph_deploy request"; "params" => format!("{:?}", params));

        let node_id = params.node_id.clone().unwrap_or(self.node_id.clone());
        let routes = subgraph_routes(&params.name, self.http_port, self.ws_port);
        match self
            .registrar
            .create_subgraph_version(
                params.name.clone(),
                params.ipfs_hash.clone(),
                node_id,
                params.debug_fork.clone(),
                // Here it doesn't make sense to receive another
                // startBlock, we'll use the one from the manifest.
                None,
                None,
                params.history_blocks,
            )
            .await
        {
            Ok(_) => Ok(routes),
            Err(e) => Err(json_rpc_error(
                &self.logger,
                "subgraph_deploy",
                e,
                Self::DEPLOY_ERROR,
                params,
            )),
        }
    }

    /// Handler for the `subgraph_remove` endpoint.
    async fn remove_handler(&self, params: SubgraphRemoveParams) -> JsonRpcResult<GraphValue> {
        info!(&self.logger, "Received subgraph_remove request"; "params" => format!("{:?}", params));

        match self.registrar.remove_subgraph(params.name.clone()).await {
            Ok(_) => Ok(Value::Null),
            Err(e) => Err(json_rpc_error(
                &self.logger,
                "subgraph_remove",
                e,
                Self::REMOVE_ERROR,
                params,
            )),
        }
    }

    /// Handler for the `subgraph_assign` endpoint.
    async fn reassign_handler(&self, params: SubgraphReassignParams) -> JsonRpcResult<GraphValue> {
        info!(&self.logger, "Received subgraph_reassignment request"; "params" => format!("{:?}", params));

        match self
            .registrar
            .reassign_subgraph(&params.ipfs_hash, &params.node_id)
            .await
        {
            Ok(_) => Ok(Value::Null),
            Err(e) => Err(json_rpc_error(
                &self.logger,
                "subgraph_reassign",
                e,
                Self::REASSIGN_ERROR,
                params,
            )),
        }
    }
}

fn json_rpc_error(
    logger: &Logger,
    operation: &str,
    e: SubgraphRegistrarError,
    code: i64,
    params: impl std::fmt::Debug,
) -> JsonRpcError {
    error!(logger, "{} failed", operation;
        "error" => format!("{:?}", e),
        "params" => format!("{:?}", params));

    let message = if let SubgraphRegistrarError::Unknown(_) = e {
        "internal error".to_owned()
    } else {
        e.to_string()
    };

    JsonRpcError::Call(CallError::Custom(ErrorObject::owned(
        code as _,
        message,
        None::<String>,
    )))
}

fn subgraph_routes(name: &SubgraphName, http_port: u16, ws_port: u16) -> JsonValue {
    let http_base_url = ENV_VARS
        .external_http_base_url
        .clone()
        .unwrap_or_else(|| format!(":{}", http_port));
    let ws_base_url = ENV_VARS
        .external_ws_base_url
        .clone()
        .unwrap_or_else(|| format!(":{}", ws_port));

    let mut map = BTreeMap::new();
    map.insert(
        "playground",
        format!("{}/subgraphs/name/{}/graphql", http_base_url, name),
    );
    map.insert(
        "queries",
        format!("{}/subgraphs/name/{}", http_base_url, name),
    );
    map.insert(
        "subscriptions",
        format!("{}/subgraphs/name/{}", ws_base_url, name),
    );

    serde_json::to_value(map).expect("invalid subgraph routes")
}

#[derive(Debug, Deserialize)]
struct SubgraphCreateParams {
    name: SubgraphName,
}

#[derive(Debug, Deserialize)]
struct SubgraphDeployParams {
    name: SubgraphName,
    ipfs_hash: DeploymentHash,
    node_id: Option<NodeId>,
    debug_fork: Option<DeploymentHash>,
    history_blocks: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct SubgraphRemoveParams {
    name: SubgraphName,
}

#[derive(Debug, Deserialize)]
struct SubgraphReassignParams {
    ipfs_hash: DeploymentHash,
    node_id: NodeId,
}
