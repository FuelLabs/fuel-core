use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
    },
    block_range_response::BlockRangeResponse,
    result::Result,
};
use async_trait::async_trait;
use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

tonic::include_proto!("blockaggregator");

use crate::result::Error;
use block_aggregator_server::BlockAggregator;

#[cfg(test)]
mod tests;

pub struct Server {
    query_sender: tokio::sync::mpsc::Sender<BlockAggregatorQuery<BlockRangeResponse>>,
}

impl Server {
    pub fn new(
        query_sender: tokio::sync::mpsc::Sender<BlockAggregatorQuery<BlockRangeResponse>>,
    ) -> Self {
        Self { query_sender }
    }
}

#[async_trait]
impl BlockAggregator for Server {
    async fn get_block_height(
        &self,
        request: tonic::Request<BlockHeightRequest>,
    ) -> Result<tonic::Response<BlockHeightResponse>, tonic::Status> {
        tracing::debug!("get_block_height: {:?}", request);
        let (response, receiver) = tokio::sync::oneshot::channel();
        let query = BlockAggregatorQuery::GetCurrentHeight { response };
        self.query_sender.send(query).await.map_err(|e| {
            tonic::Status::internal(format!("Failed to send query: {}", e))
        })?;
        let res = receiver.await;
        match res {
            Ok(height) => Ok(tonic::Response::new(BlockHeightResponse {
                height: *height,
            })),
            Err(e) => Err(tonic::Status::internal(format!(
                "Failed to receive height: {}",
                e
            ))),
        }
    }
    type GetBlockRangeStream = ReceiverStream<Result<Block, Status>>;

    async fn get_block_range(
        &self,
        request: tonic::Request<BlockRangeRequest>,
    ) -> Result<tonic::Response<Self::GetBlockRangeStream>, tonic::Status> {
        tracing::warn!("get_block_range: {:?}", request);
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
                    let (tx, rx) =
                        tokio::sync::mpsc::channel::<Result<Block, Status>>(16);

                    // TODO: is this safe if we just drop the join handle?
                    let _ = tokio::spawn(async move {
                        let mut s = inner;
                        while let Some(block) = s.next().await {
                            // Convert your internal `blocks::Block` into the protobuf `Block`.
                            let pb = Block {
                                data: block.bytes().to_vec(),
                            };

                            // If the receiver side was dropped, stop forwarding.
                            if tx.send(Ok(pb)).await.is_err() {
                                break;
                            }
                        }
                    });

                    Ok(tonic::Response::new(ReceiverStream::new(rx)))
                }
                BlockRangeResponse::Remote(_) => {
                    tracing::error!("Remote block range not implemented");
                    todo!()
                }
            },
            Err(e) => Err(tonic::Status::internal(format!(
                "Failed to receive block range: {}",
                e
            ))),
        }
    }

    type NewBlockSubscriptionStream = ReceiverStream<Result<Block, Status>>;

    async fn new_block_subscription(
        &self,
        request: tonic::Request<NewBlockSubscriptionRequest>,
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
        let _ = tokio::spawn(async move {
            while let Some(nb) = receiver.recv().await {
                let block = Block {
                    data: nb.block.bytes().to_vec(),
                };
                if task_sender.send(Ok(block)).await.is_err() {
                    break;
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(task_receiver)))
    }
}

pub struct ProtobufAPI {
    _server_task_handle: tokio::task::JoinHandle<()>,
    query_receiver: tokio::sync::mpsc::Receiver<BlockAggregatorQuery<BlockRangeResponse>>,
}

impl ProtobufAPI {
    pub fn new(url: String) -> Self {
        let (query_sender, query_receiver) =
            tokio::sync::mpsc::channel::<BlockAggregatorQuery<BlockRangeResponse>>(100);
        let server = Server::new(query_sender);
        let addr = url.parse().unwrap();
        let _server_task_handle = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(block_aggregator_server::BlockAggregatorServer::new(server))
                .serve(addr)
                .await
                .unwrap();
        });
        Self {
            _server_task_handle,
            query_receiver,
        }
    }
}

impl BlockAggregatorApi for ProtobufAPI {
    type BlockRangeResponse = BlockRangeResponse;

    async fn await_query(
        &mut self,
    ) -> Result<BlockAggregatorQuery<Self::BlockRangeResponse>> {
        let query = self
            .query_receiver
            .recv()
            .await
            .ok_or_else(|| Error::Api(anyhow::anyhow!("Channel closed")))?;
        Ok(query)
    }
}

pub struct ProtobufClient;
