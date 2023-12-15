use std::{marker::PhantomData, sync::Arc, time::Duration};

use crate::{
    blockchain::Block as BlockchainBlock,
    components::store::ChainStore,
    firehose::{self, decode_firehose_block, HeaderOnly},
    prelude::{error, info, Logger},
    util::backoff::ExponentialBackoff,
};
use anyhow::{Context, Error};
use async_trait::async_trait;
use futures03::StreamExt;
use prost::Message;
use prost_types::Any;
use slog::{o, trace};
use tonic::Streaming;

use super::{client::ChainClient, BlockIngestor, Blockchain};

const TRANSFORM_ETHEREUM_HEADER_ONLY: &str =
    "type.googleapis.com/sf.ethereum.transform.v1.HeaderOnly";

pub enum Transforms {
    EthereumHeaderOnly,
}

impl From<&Transforms> for Any {
    fn from(val: &Transforms) -> Self {
        match val {
            Transforms::EthereumHeaderOnly => Any {
                type_url: TRANSFORM_ETHEREUM_HEADER_ONLY.to_owned(),
                value: HeaderOnly {}.encode_to_vec(),
            },
        }
    }
}

pub struct FirehoseBlockIngestor<M, C: Blockchain>
where
    M: prost::Message + BlockchainBlock + Default + 'static,
{
    chain_store: Arc<dyn ChainStore>,
    client: Arc<ChainClient<C>>,
    logger: Logger,
    default_transforms: Vec<Transforms>,
    chain_name: String,

    phantom: PhantomData<M>,
}

impl<M, C: Blockchain> FirehoseBlockIngestor<M, C>
where
    M: prost::Message + BlockchainBlock + Default + 'static,
{
    pub fn new(
        chain_store: Arc<dyn ChainStore>,
        client: Arc<ChainClient<C>>,
        logger: Logger,
        chain_name: String,
    ) -> FirehoseBlockIngestor<M, C> {
        FirehoseBlockIngestor {
            chain_store,
            client,
            logger,
            phantom: PhantomData {},
            default_transforms: vec![],
            chain_name,
        }
    }

    pub fn with_transforms(mut self, transforms: Vec<Transforms>) -> Self {
        self.default_transforms = transforms;
        self
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
        mut stream: Streaming<firehose::Response>,
    ) -> String {
        use firehose::ForkStep;
        use firehose::ForkStep::*;

        let mut latest_cursor = cursor;

        while let Some(message) = stream.next().await {
            match message {
                Ok(v) => {
                    let step = ForkStep::from_i32(v.step)
                        .expect("Fork step should always match to known value");

                    let result = match step {
                        StepNew => self.process_new_block(&v).await,
                        StepUndo => {
                            trace!(self.logger, "Received undo block to ingest, skipping");
                            Ok(())
                        }
                        StepFinal | StepUnset => panic!(
                            "We explicitly requested StepNew|StepUndo but received something else"
                        ),
                    };

                    if let Err(e) = result {
                        error!(self.logger, "Process block failed: {:#}", e);
                        break;
                    }

                    latest_cursor = v.cursor;
                }
                Err(e) => {
                    info!(
                        self.logger,
                        "An error occurred while streaming blocks: {}", e
                    );
                    break;
                }
            }
        }

        error!(
            self.logger,
            "Stream blocks complete unexpectedly, expecting stream to always stream blocks"
        );
        latest_cursor
    }

    async fn process_new_block(&self, response: &firehose::Response) -> Result<(), Error> {
        let block = decode_firehose_block::<M>(response)
            .context("Mapping firehose block to blockchain::Block")?;

        trace!(self.logger, "Received new block to ingest {}", block.ptr());

        self.chain_store
            .clone()
            .set_chain_head(block, response.cursor.clone())
            .await
            .context("Updating chain head")?;

        Ok(())
    }
}

#[async_trait]
impl<M, C: Blockchain> BlockIngestor for FirehoseBlockIngestor<M, C>
where
    M: prost::Message + BlockchainBlock + Default + 'static,
{
    async fn run(self: Box<Self>) {
        let mut latest_cursor = self.fetch_head_cursor().await;
        let mut backoff =
            ExponentialBackoff::new(Duration::from_millis(250), Duration::from_secs(30));

        loop {
            let endpoint = match self.client.firehose_endpoint() {
                Ok(endpoint) => endpoint,
                Err(err) => {
                    error!(
                        self.logger,
                        "Unable to get a connection for block ingestor, err: {}", err
                    );
                    backoff.sleep_async().await;
                    continue;
                }
            };

            let logger = self.logger.new(
                o!("provider" => endpoint.provider.to_string(), "network_name"=> self.network_name()),
            );

            info!(
                logger,
                "Trying to reconnect the Blockstream after disconnect"; "endpoint uri" => format_args!("{}", endpoint), "cursor" => format_args!("{}", latest_cursor),
            );

            let result = endpoint
                .clone()
                .stream_blocks(firehose::Request {
                    // Starts at current HEAD block of the chain (viewed from Firehose side)
                    start_block_num: -1,
                    cursor: latest_cursor.clone(),
                    final_blocks_only: false,
                    transforms: self.default_transforms.iter().map(|t| t.into()).collect(),
                    ..Default::default()
                })
                .await;

            match result {
                Ok(stream) => {
                    info!(logger, "Blockstream connected, consuming blocks");

                    // Consume the stream of blocks until an error is hit
                    latest_cursor = self.process_blocks(latest_cursor, stream).await
                }
                Err(e) => {
                    error!(logger, "Unable to connect to endpoint: {:#}", e);
                }
            }

            // If we reach this point, we must wait a bit before retrying
            backoff.sleep_async().await;
        }
    }

    fn network_name(&self) -> String {
        self.chain_name.clone()
    }
}
