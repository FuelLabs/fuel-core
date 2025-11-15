use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
    },
    block_range_response::{
        BlockRangeResponse,
        BoxStream,
    },
    protobuf_types::{
        Block as ProtoBlock,
        BlockHeightRequest as ProtoBlockHeightRequest,
        BlockHeightResponse as ProtoBlockHeightResponse,
        BlockRangeRequest as ProtoBlockRangeRequest,
        BlockResponse as ProtoBlockResponse,
        NewBlockSubscriptionRequest as ProtoNewBlockSubscriptionRequest,
        RemoteBlockRangeResponse as ProtoRemoteBlockRangeResponse,
        block_aggregator_server::{
            BlockAggregator,
            BlockAggregatorServer as ProtoBlockAggregatorServer,
        },
        block_response as proto_block_response,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

#[cfg(test)]
mod tests;

pub struct Server {
    query_sender:
        tokio::sync::mpsc::Sender<BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>>,
}

impl Server {
    pub fn new(
        query_sender: tokio::sync::mpsc::Sender<
            BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>,
        >,
    ) -> Self {
        Self { query_sender }
    }
}

#[async_trait]
impl BlockAggregator for Server {
    async fn get_block_height(
        &self,
        request: tonic::Request<ProtoBlockHeightRequest>,
    ) -> Result<tonic::Response<ProtoBlockHeightResponse>, tonic::Status> {
        tracing::debug!("get_block_height: {:?}", request);
        tracing::info!("get_block_height: {:?}", request);
        let (response, receiver) = tokio::sync::oneshot::channel();
        let query = BlockAggregatorQuery::GetCurrentHeight { response };
        self.query_sender.send(query).await.map_err(|e| {
            tonic::Status::internal(format!("Failed to send query: {}", e))
        })?;
        let res = receiver.await;
        tracing::info!("query result: {:?}", &res);
        match res {
            Ok(height) => Ok(tonic::Response::new(ProtoBlockHeightResponse {
                height: height.map(|inner| *inner),
            })),
            Err(e) => Err(tonic::Status::internal(format!(
                "Failed to receive height: {}",
                e
            ))),
        }
    }
    // type GetBlockRangeStream = ReceiverStream<Result<ProtoBlockResponse, Status>>;
    type GetBlockRangeStream = BoxStream<Result<ProtoBlockResponse, Status>>;

    async fn get_block_range(
        &self,
        request: tonic::Request<ProtoBlockRangeRequest>,
    ) -> Result<tonic::Response<Self::GetBlockRangeStream>, tonic::Status> {
        let req = request.into_inner();
        let (response, receiver) = tokio::sync::oneshot::channel();
        let query = BlockAggregatorQuery::GetBlockRange {
            first: req.start.into(),
            last: req.end.into(),
            response,
        };
        self.query_sender
            .send(query)
            .await
            .map_err(|e| Status::internal(format!("Failed to send query: {}", e)))?;
        let res = receiver.await;
        match res {
            Ok(block_range_response) => match block_range_response {
                BlockRangeResponse::Literal(inner) => {
                    let stream = inner
                        .map(|res| {
                            let response = ProtoBlockResponse {
                                payload: Some(proto_block_response::Payload::Literal(
                                    res,
                                )),
                            };
                            Ok(response)
                        })
                        .boxed();
                    Ok(tonic::Response::new(stream))
                }
                BlockRangeResponse::Remote(inner) => {
                    let stream = inner
                        .map(|res| {
                            let proto_response = ProtoRemoteBlockRangeResponse {
                                region: res.region.clone(),
                                bucket: res.bucket.clone(),
                                key: res.key.clone(),
                                url: res.url.clone(),
                            };
                            let response = ProtoBlockResponse {
                                payload: Some(proto_block_response::Payload::Remote(
                                    proto_response,
                                )),
                            };
                            Ok(response)
                        })
                        .boxed();
                    Ok(tonic::Response::new(stream))
                }
            },
            Err(e) => Err(tonic::Status::internal(format!(
                "Failed to receive block range: {}",
                e
            ))),
        }
    }

    type NewBlockSubscriptionStream = ReceiverStream<Result<ProtoBlockResponse, Status>>;

    async fn new_block_subscription(
        &self,
        request: tonic::Request<ProtoNewBlockSubscriptionRequest>,
    ) -> Result<tonic::Response<Self::NewBlockSubscriptionStream>, tonic::Status> {
        const ARB_CHANNEL_SIZE: usize = 100;
        tracing::warn!("get_block_range: {:?}", request);
        let (response, mut receiver) = tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        let query = BlockAggregatorQuery::NewBlockSubscription { response };
        self.query_sender
            .send(query)
            .await
            .map_err(|e| Status::internal(format!("Failed to send query: {}", e)))?;

        let (task_sender, task_receiver) = tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        tokio::spawn(async move {
            while let Some(nb) = receiver.recv().await {
                let response = ProtoBlockResponse {
                    payload: Some(proto_block_response::Payload::Literal(nb)),
                };
                if task_sender.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(task_receiver)))
    }
}

pub struct ProtobufAPI {
    _server_service: ServiceRunner<ServerTask>,
    query_receiver:
        tokio::sync::mpsc::Receiver<BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>>,
}

pub struct ServerTask {
    addr: std::net::SocketAddr,
    query_sender:
        tokio::sync::mpsc::Sender<BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>>,
}
#[async_trait::async_trait]
impl RunnableService for ServerTask {
    const NAME: &'static str = "ProtobufServerTask";
    type SharedData = ();
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl RunnableTask for ServerTask {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        let server = Server::new(self.query_sender.clone());
        let router = tonic::transport::Server::builder()
            .add_service(ProtoBlockAggregatorServer::new(server));
        tokio::select! {
                res = router.serve(self.addr) => {
                    if let Err(e) = res {
                        tracing::error!("BlockAggregator tonic server error: {}", e);
                        TaskNextAction::ErrorContinue(anyhow!(e))
                    } else {
                        tracing::info!("BlockAggregator tonic server stopped");
                        TaskNextAction::Stop
                    }
                },
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl ProtobufAPI {
    pub fn new(url: String) -> Result<Self> {
        let (query_sender, query_receiver) = tokio::sync::mpsc::channel::<
            BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>,
        >(100);
        let addr = url.parse().unwrap();
        let _server_service = ServiceRunner::new(ServerTask { addr, query_sender });
        _server_service.start().map_err(|e| Error::Api(e))?;
        let api = Self {
            _server_service,
            query_receiver,
        };
        Ok(api)
    }
}

impl BlockAggregatorApi for ProtobufAPI {
    type BlockRangeResponse = BlockRangeResponse;
    type Block = ProtoBlock;

    async fn await_query(
        &mut self,
    ) -> Result<BlockAggregatorQuery<Self::BlockRangeResponse, Self::Block>> {
        let query = self
            .query_receiver
            .recv()
            .await
            .ok_or_else(|| Error::Api(anyhow::anyhow!("Channel closed")))?;
        Ok(query)
    }
}
