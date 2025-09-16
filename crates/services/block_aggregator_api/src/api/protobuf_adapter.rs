use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
    },
    block_range_response::BlockRangeResponse,
    result::Result,
};
use async_trait::async_trait;

tonic::include_proto!("blockaggregator");

use crate::result::Error;
use block_aggregator_server::BlockAggregator;

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
}

pub struct ProtobufAPI {
    server_task_handle: tokio::task::JoinHandle<()>,
    query_receiver: tokio::sync::mpsc::Receiver<BlockAggregatorQuery<BlockRangeResponse>>,
}

impl ProtobufAPI {
    pub fn new(url: String) -> Self {
        let (query_sender, query_receiver) =
            tokio::sync::mpsc::channel::<BlockAggregatorQuery<BlockRangeResponse>>(100);
        let server = Server::new(query_sender);
        let addr = url.parse().unwrap();
        let server_task_handle = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(block_aggregator_server::BlockAggregatorServer::new(server))
                .serve(addr)
                .await
                .unwrap();
        });
        Self {
            server_task_handle,
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

#[cfg(test)]
mod tests {
    use super::*;
    use block_aggregator_client::BlockAggregatorClient;
    use fuel_core_types::fuel_types::BlockHeight;

    #[tokio::test]
    async fn await_query__client_receives_expected_value() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        // given
        let path = "[::1]:50051";
        let mut api = ProtobufAPI::new(path.to_string());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // call get current height endpoint with client
        let url = "http://[::1]:50051";
        let mut client = BlockAggregatorClient::connect(url.to_string())
            .await
            .expect("could not connect to server");
        let handle = tokio::spawn(async move {
            tracing::info!("querying with client");
            client
                .get_block_height(BlockHeightRequest {})
                .await
                .expect("could not get height")
        });

        // when
        tracing::info!("awaiting query");
        let query = api.await_query().await.unwrap();

        // then
        // return response through query's channel
        if let BlockAggregatorQuery::GetCurrentHeight { response } = query {
            response.send(BlockHeight::new(42)).unwrap();
        } else {
            panic!("expected GetCurrentHeight query");
        }
        let res = handle.await.unwrap();

        // assert client received expected value
        assert_eq!(res.into_inner().height, 42);
    }
}
