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
use async_trait::async_trait;
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
    _server_task_handle: tokio::task::JoinHandle<()>,
    shutdown_sender: Option<tokio::sync::oneshot::Sender<()>>,
    query_receiver:
        tokio::sync::mpsc::Receiver<BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>>,
}

impl ProtobufAPI {
    pub fn new(url: String) -> Self {
        let (query_sender, query_receiver) = tokio::sync::mpsc::channel::<
            BlockAggregatorQuery<BlockRangeResponse, ProtoBlock>,
        >(100);
        let server = Server::new(query_sender);
        let addr = url.parse().unwrap();
        let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel::<()>();
        let _server_task_handle = tokio::spawn(async move {
            let service = tonic::transport::Server::builder()
                .add_service(ProtoBlockAggregatorServer::new(server));
            tokio::select! {
                res = service.serve(addr) => {
                    if let Err(e) = res {
                        tracing::error!("BlockAggregator tonic server error: {}", e);
                    } else {
                        tracing::info!("BlockAggregator tonic server stopped");
                    }
                },
                _ = shutdown_receiver => {
                    tracing::info!("Shutting down BlockAggregator tonic server");
                },
            }
        });
        Self {
            _server_task_handle,
            shutdown_sender: Some(shutdown_sender),
            query_receiver,
        }
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

impl Drop for ProtobufAPI {
    fn drop(&mut self) {
        if let Some(shutdown_sender) = self.shutdown_sender.take() {
            let _ = shutdown_sender.send(());
        }
    }
}
