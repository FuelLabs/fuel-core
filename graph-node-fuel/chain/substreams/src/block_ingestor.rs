use std::{sync::Arc, time::Duration};

use crate::mapper::Mapper;
use anyhow::{Context, Error};
use graph::blockchain::{
    client::ChainClient, substreams_block_stream::SubstreamsBlockStream, BlockIngestor,
};
use graph::prelude::MetricsRegistry;
use graph::slog::trace;
use graph::substreams::Package;
use graph::tokio_stream::StreamExt;
use graph::{
    blockchain::block_stream::BlockStreamEvent,
    cheap_clone::CheapClone,
    components::store::ChainStore,
    prelude::{async_trait, error, info, DeploymentHash, Logger},
    util::backoff::ExponentialBackoff,
};
use prost::Message;

const SUBSTREAMS_HEAD_TRACKER_BYTES: &[u8; 89935] = include_bytes!(
    "../../../substreams/substreams-head-tracker/substreams-head-tracker-v1.0.0.spkg"
);

pub struct SubstreamsBlockIngestor {
    chain_store: Arc<dyn ChainStore>,
    client: Arc<ChainClient<super::Chain>>,
    logger: Logger,
    chain_name: String,
    metrics: Arc<MetricsRegistry>,
}

impl SubstreamsBlockIngestor {
    pub fn new(
        chain_store: Arc<dyn ChainStore>,
        client: Arc<ChainClient<super::Chain>>,
        logger: Logger,
        chain_name: String,
        metrics: Arc<MetricsRegistry>,
    ) -> SubstreamsBlockIngestor {
        SubstreamsBlockIngestor {
            chain_store,
            client,
            logger,
            chain_name,
            metrics,
        }
    }

    async fn fetch_head_cursor(&self) -> String {
        let mut backoff =
            ExponentialBackoff::new(Duration::from_millis(250), Duration::from_secs(30));
        loop {
            match self.chain_store.clone().chain_head_cursor() {
                Ok(cursor) => return cursor.unwrap_or_default(),
                Err(e) => {
                    error!(self.logger, "Fetching chain head cursor failed: {:#}", e);

                    backoff.sleep_async().await;
                }
            }
        }
    }

    /// Consumes the incoming stream of blocks infinitely until it hits an error. In which case
    /// the error is logged right away and the latest available cursor is returned
    /// upstream for future consumption.
    async fn process_blocks(
        &self,
        cursor: String,
        mut stream: SubstreamsBlockStream<super::Chain>,
    ) -> String {
        let mut latest_cursor = cursor;

        while let Some(message) = stream.next().await {
            let (block, cursor) = match message {
                Ok(BlockStreamEvent::ProcessWasmBlock(_block_ptr, _data, _handler, _cursor)) => {
                    unreachable!("Block ingestor should never receive raw blocks");
                }
                Ok(BlockStreamEvent::ProcessBlock(triggers, cursor)) => {
                    (Arc::new(triggers.block), cursor)
                }
                Ok(BlockStreamEvent::Revert(_ptr, _cursor)) => {
                    trace!(self.logger, "Received undo block to ingest, skipping");
                    continue;
                }
                Err(e) => {
                    info!(
                        self.logger,
                        "An error occurred while streaming blocks: {}", e
                    );
                    break;
                }
            };

            let res = self.process_new_block(block, cursor.to_string()).await;
            if let Err(e) = res {
                error!(self.logger, "Process block failed: {:#}", e);
                break;
            }

            latest_cursor = cursor.to_string()
        }

        error!(
            self.logger,
            "Stream blocks complete unexpectedly, expecting stream to always stream blocks"
        );
        latest_cursor
    }

    async fn process_new_block(
        &self,
        block: Arc<super::Block>,
        cursor: String,
    ) -> Result<(), Error> {
        trace!(self.logger, "Received new block to ingest {:?}", block);

        self.chain_store
            .clone()
            .set_chain_head(block, cursor)
            .await
            .context("Updating chain head")?;

        Ok(())
    }
}

#[async_trait]
impl BlockIngestor for SubstreamsBlockIngestor {
    async fn run(self: Box<Self>) {
        let mapper = Arc::new(Mapper {
            schema: None,
            skip_empty_blocks: false,
        });
        let mut latest_cursor = self.fetch_head_cursor().await;
        let mut backoff =
            ExponentialBackoff::new(Duration::from_millis(250), Duration::from_secs(30));
        let package = Package::decode(SUBSTREAMS_HEAD_TRACKER_BYTES.to_vec().as_ref()).unwrap();

        loop {
            let stream = SubstreamsBlockStream::<super::Chain>::new(
                DeploymentHash::default(),
                self.client.cheap_clone(),
                None,
                Some(latest_cursor.clone()),
                mapper.cheap_clone(),
                package.modules.clone(),
                "map_blocks".to_string(),
                vec![-1],
                vec![],
                self.logger.cheap_clone(),
                self.metrics.cheap_clone(),
            );

            // Consume the stream of blocks until an error is hit
            latest_cursor = self.process_blocks(latest_cursor, stream).await;

            // If we reach this point, we must wait a bit before retrying
            backoff.sleep_async().await;
        }
    }

    fn network_name(&self) -> String {
        self.chain_name.clone()
    }
}
