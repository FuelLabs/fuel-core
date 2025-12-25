use crate::{
    block_range_response::{
        BlockRangeResponse,
        BoxStream,
    },
    protobuf_types::{
        BlockHeightRequest as ProtoBlockHeightRequest,
        BlockHeightResponse as ProtoBlockHeightResponse,
        BlockRangeRequest as ProtoBlockRangeRequest,
        BlockResponse as ProtoBlockResponse,
        NewBlockSubscriptionRequest as ProtoNewBlockSubscriptionRequest,
        RemoteBlockResponse as ProtoRemoteBlockResponse,
        RemoteS3Bucket as ProtoRemoteS3Bucket,
        block_aggregator_server::{
            BlockAggregator,
            BlockAggregatorServer as ProtoBlockAggregatorServer,
        },
        block_response as proto_block_response,
        remote_block_response::Location as ProtoRemoteLocation,
    },
    result::Result,
};
use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
    stream::Stream,
};
use fuel_core_types::fuel_types::BlockHeight;
use futures::StreamExt;
use std::{
    net::SocketAddr,
    sync::Arc,
};
use tonic::{
    Status,
    transport::server::Router,
};

#[cfg(test)]
mod tests;

#[cfg_attr(test, mockall::automock)]
pub trait BlocksAggregatorApi: Send + Sync + 'static {
    fn get_block_range<H>(&self, first: H, last: H) -> Result<BlockRangeResponse>
    where
        H: Into<BlockHeight> + 'static;

    fn get_current_height(&self) -> Result<Option<BlockHeight>>;

    // TODO: This doesn't actually need to be bytes, it could just be the ProtoBlock type since we
    //   don't need to deserialize it, but this works for now
    fn new_block_subscription(
        &self,
    ) -> impl Stream<Item = anyhow::Result<(BlockHeight, Arc<[u8]>)>> + Send + 'static;
}

pub struct Server<B> {
    api: B,
}

impl<B> Server<B> {
    pub fn new(api: B) -> Self {
        Self { api }
    }
}

#[async_trait]
impl<B> BlockAggregator for Server<B>
where
    B: BlocksAggregatorApi,
{
    async fn get_synced_block_height(
        &self,
        _: tonic::Request<ProtoBlockHeightRequest>,
    ) -> Result<tonic::Response<ProtoBlockHeightResponse>, tonic::Status> {
        let res = self.api.get_current_height();
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
    type GetBlockRangeStream = BoxStream<Result<ProtoBlockResponse, Status>>;

    async fn get_block_range(
        &self,
        request: tonic::Request<ProtoBlockRangeRequest>,
    ) -> Result<tonic::Response<Self::GetBlockRangeStream>, tonic::Status> {
        let req = request.into_inner();
        let res = self.api.get_block_range(req.start, req.end);
        match res {
            Ok(block_range_response) => match block_range_response {
                BlockRangeResponse::Literal(inner) => {
                    let stream = inner
                        .map(|(height, res)| {
                            let response = ProtoBlockResponse {
                                height: *height,
                                payload: Some(proto_block_response::Payload::Literal(
                                    res,
                                )),
                            };
                            Ok(response)
                        })
                        .boxed();
                    Ok(tonic::Response::new(stream))
                }
                BlockRangeResponse::Bytes(inner) => {
                    let stream = inner
                        .map(|(height, res)| {
                            let response = ProtoBlockResponse {
                                height: *height,
                                payload: Some(proto_block_response::Payload::Bytes(
                                    (*res).to_vec(),
                                )),
                            };
                            Ok(response)
                        })
                        .boxed();
                    Ok(tonic::Response::new(stream))
                }
                BlockRangeResponse::S3(inner) => {
                    let stream = inner
                        .map(|(height, res)| {
                            let s3 = ProtoRemoteS3Bucket {
                                bucket: res.bucket,
                                key: res.key,
                                requester_pays: res.requester_pays,
                                endpoint: res.aws_endpoint,
                            };
                            let location = ProtoRemoteLocation::S3(s3);
                            let proto_response = ProtoRemoteBlockResponse {
                                location: Some(location),
                            };
                            let response = ProtoBlockResponse {
                                height: *height,
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

    type NewBlockSubscriptionStream = BoxStream<Result<ProtoBlockResponse, Status>>;

    async fn new_block_subscription(
        &self,
        _: tonic::Request<ProtoNewBlockSubscriptionRequest>,
    ) -> Result<tonic::Response<Self::NewBlockSubscriptionStream>, tonic::Status> {
        let stream = self.api.new_block_subscription().map(|item| {
            match item {
                Ok((block_height, block)) => {
                    let response = ProtoBlockResponse {
                        height: *block_height,
                        // TODO: Avoid clone
                        payload: Some(proto_block_response::Payload::Bytes(
                            block.to_vec(),
                        )),
                    };
                    Ok(response)
                }
                Err(err) => Err(tonic::Status::internal(format!(
                    "Failed to receive new block: {}",
                    err
                ))),
            }
        });

        Ok(tonic::Response::new(stream.boxed()))
    }
}

pub struct UninitializedTask<B> {
    addr: SocketAddr,
    api: B,
}

pub struct Task {
    addr: SocketAddr,
    router: Option<Router>,
}

#[async_trait::async_trait]
impl<B> RunnableService for UninitializedTask<B>
where
    B: BlocksAggregatorApi,
{
    const NAME: &'static str = "ProtobufServerTask";
    type SharedData = ();
    type Task = Task;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(
        mut self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let server = Server::new(self.api);
        let router = tonic::transport::Server::builder()
            .add_service(ProtoBlockAggregatorServer::new(server));
        let task = Task {
            addr: self.addr,
            router: Some(router),
        };
        Ok(task)
    }
}

impl RunnableTask for Task {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        let mut watcher_local = watcher.clone();
        let future = self
            .router
            .take()
            .expect("Router is always initialized; qed")
            .serve_with_shutdown(self.addr, async move {
                let _ = watcher_local.while_started().await;
            });

        tokio::select! {
            res = future => {
                if let Err(e) = res {
                    tracing::error!("BlockAggregator tonic server error: {}", e);
                } else {
                    tracing::info!("BlockAggregator tonic server stopped");
                }
            },
            _ = watcher.while_started() => {}
        }
        TaskNextAction::Stop
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub type APIService<B> = ServiceRunner<UninitializedTask<B>>;

pub fn new_service<B>(addr: SocketAddr, api: B) -> APIService<B>
where
    B: BlocksAggregatorApi,
{
    ServiceRunner::new(UninitializedTask { addr, api })
}
