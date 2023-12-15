use crate::substreams::Clock;
use crate::substreams_rpc::response::Message as SubstreamsMessage;
use crate::substreams_rpc::BlockScopedData;
use anyhow::Error;
use async_stream::stream;
use futures03::Stream;
use prost_types::Any;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};

use super::substreams_block_stream::SubstreamsLogData;
use super::{Block, BlockPtr, Blockchain};
use crate::anyhow::Result;
use crate::components::store::{BlockNumber, DeploymentLocator};
use crate::data::subgraph::UnifiedMappingApiVersion;
use crate::firehose::{self, FirehoseEndpoint};
use crate::schema::InputSchema;
use crate::substreams_rpc::response::Message;
use crate::{prelude::*, prometheus::labels};

pub const BUFFERED_BLOCK_STREAM_SIZE: usize = 100;
pub const FIREHOSE_BUFFER_STREAM_SIZE: usize = 1;
pub const SUBSTREAMS_BUFFER_STREAM_SIZE: usize = 1;

pub struct BufferedBlockStream<C: Blockchain> {
    inner: Pin<Box<dyn Stream<Item = Result<BlockStreamEvent<C>, Error>> + Send>>,
}

impl<C: Blockchain + 'static> BufferedBlockStream<C> {
    pub fn spawn_from_stream(
        size_hint: usize,
        stream: Box<dyn BlockStream<C>>,
    ) -> Box<dyn BlockStream<C>> {
        let (sender, receiver) = mpsc::channel::<Result<BlockStreamEvent<C>, Error>>(size_hint);
        crate::spawn(async move { BufferedBlockStream::stream_blocks(stream, sender).await });

        Box::new(BufferedBlockStream::new(receiver))
    }

    pub fn new(mut receiver: Receiver<Result<BlockStreamEvent<C>, Error>>) -> Self {
        let inner = stream! {
            loop {
                let event = match receiver.recv().await {
                    Some(evt) => evt,
                    None => return,
                };

                yield event
            }
        };

        Self {
            inner: Box::pin(inner),
        }
    }

    pub async fn stream_blocks(
        mut stream: Box<dyn BlockStream<C>>,
        sender: Sender<Result<BlockStreamEvent<C>, Error>>,
    ) -> Result<(), Error> {
        while let Some(event) = stream.next().await {
            match sender.send(event).await {
                Ok(_) => continue,
                Err(err) => {
                    return Err(anyhow!(
                        "buffered blockstream channel is closed, stopping. Err: {}",
                        err
                    ))
                }
            }
        }

        Ok(())
    }
}

impl<C: Blockchain> BlockStream<C> for BufferedBlockStream<C> {
    fn buffer_size_hint(&self) -> usize {
        unreachable!()
    }
}

impl<C: Blockchain> Stream for BufferedBlockStream<C> {
    type Item = Result<BlockStreamEvent<C>, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

pub trait BlockStream<C: Blockchain>:
    Stream<Item = Result<BlockStreamEvent<C>, Error>> + Unpin + Send
{
    fn buffer_size_hint(&self) -> usize;
}

/// BlockRefetcher abstraction allows a chain to decide if a block must be refetched after a dynamic data source was added
#[async_trait]
pub trait BlockRefetcher<C: Blockchain>: Send + Sync {
    fn required(&self, chain: &C) -> bool;

    async fn get_block(
        &self,
        chain: &C,
        logger: &Logger,
        cursor: FirehoseCursor,
    ) -> Result<C::Block, Error>;
}

/// BlockStreamBuilder is an abstraction that would separate the logic for building streams from the blockchain trait
#[async_trait]
pub trait BlockStreamBuilder<C: Blockchain>: Send + Sync {
    async fn build_firehose(
        &self,
        chain: &C,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        start_blocks: Vec<BlockNumber>,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<C::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<C>>>;

    async fn build_substreams(
        &self,
        chain: &C,
        schema: InputSchema,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<C::TriggerFilter>,
    ) -> Result<Box<dyn BlockStream<C>>>;

    async fn build_polling(
        &self,
        chain: &C,
        deployment: DeploymentLocator,
        start_blocks: Vec<BlockNumber>,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<C::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<C>>>;
}

#[derive(Debug, Clone)]
pub struct FirehoseCursor(Option<String>);

impl FirehoseCursor {
    #[allow(non_upper_case_globals)]
    pub const None: Self = FirehoseCursor(None);

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Display for FirehoseCursor {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.write_str(self.0.as_deref().unwrap_or(""))
    }
}

impl From<String> for FirehoseCursor {
    fn from(cursor: String) -> Self {
        // Treat a cursor of "" as None, not absolutely necessary for correctness since the firehose
        // treats both as the same, but makes it a little clearer.
        if cursor.is_empty() {
            FirehoseCursor::None
        } else {
            FirehoseCursor(Some(cursor))
        }
    }
}

impl From<Option<String>> for FirehoseCursor {
    fn from(cursor: Option<String>) -> Self {
        match cursor {
            None => FirehoseCursor::None,
            Some(s) => FirehoseCursor::from(s),
        }
    }
}

impl AsRef<Option<String>> for FirehoseCursor {
    fn as_ref(&self) -> &Option<String> {
        &self.0
    }
}

#[derive(Debug)]
pub struct BlockWithTriggers<C: Blockchain> {
    pub block: C::Block,
    pub trigger_data: Vec<C::TriggerData>,
}

impl<C: Blockchain> Clone for BlockWithTriggers<C>
where
    C::TriggerData: Clone,
{
    fn clone(&self) -> Self {
        Self {
            block: self.block.clone(),
            trigger_data: self.trigger_data.clone(),
        }
    }
}

impl<C: Blockchain> BlockWithTriggers<C> {
    /// Creates a BlockWithTriggers structure, which holds
    /// the trigger data ordered and without any duplicates.
    pub fn new(block: C::Block, mut trigger_data: Vec<C::TriggerData>, logger: &Logger) -> Self {
        // This is where triggers get sorted.
        trigger_data.sort();

        let old_len = trigger_data.len();

        // This is removing the duplicate triggers in the case of multiple
        // data sources fetching the same event/call/etc.
        trigger_data.dedup();

        let new_len = trigger_data.len();

        if new_len != old_len {
            debug!(
                logger,
                "Trigger data had duplicate triggers";
                "block_number" => block.number(),
                "block_hash" => block.hash().hash_hex(),
                "old_length" => old_len,
                "new_length" => new_len,
            );
        }

        Self {
            block,
            trigger_data,
        }
    }

    pub fn trigger_count(&self) -> usize {
        self.trigger_data.len()
    }

    pub fn ptr(&self) -> BlockPtr {
        self.block.ptr()
    }

    pub fn parent_ptr(&self) -> Option<BlockPtr> {
        self.block.parent_ptr()
    }
}

#[async_trait]
pub trait TriggersAdapter<C: Blockchain>: Send + Sync {
    // Return the block that is `offset` blocks before the block pointed to
    // by `ptr` from the local cache. An offset of 0 means the block itself,
    // an offset of 1 means the block's parent etc. If the block is not in
    // the local cache, return `None`
    async fn ancestor_block(
        &self,
        ptr: BlockPtr,
        offset: BlockNumber,
    ) -> Result<Option<C::Block>, Error>;

    // Returns a sequence of blocks in increasing order of block number.
    // Each block will include all of its triggers that match the given `filter`.
    // The sequence may omit blocks that contain no triggers,
    // but all returned blocks must part of a same chain starting at `chain_base`.
    // At least one block will be returned, even if it contains no triggers.
    // `step_size` is the suggested number blocks to be scanned.
    async fn scan_triggers(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        filter: &C::TriggerFilter,
    ) -> Result<Vec<BlockWithTriggers<C>>, Error>;

    // Used for reprocessing blocks when creating a data source.
    async fn triggers_in_block(
        &self,
        logger: &Logger,
        block: C::Block,
        filter: &C::TriggerFilter,
    ) -> Result<BlockWithTriggers<C>, Error>;

    /// Return `true` if the block with the given hash and number is on the
    /// main chain, i.e., the chain going back from the current chain head.
    async fn is_on_main_chain(&self, ptr: BlockPtr) -> Result<bool, Error>;

    /// Get pointer to parent of `block`. This is called when reverting `block`.
    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error>;
}

#[async_trait]
pub trait FirehoseMapper<C: Blockchain>: Send + Sync {
    fn trigger_filter(&self) -> &C::TriggerFilter;

    async fn to_block_stream_event(
        &self,
        logger: &Logger,
        response: &firehose::Response,
    ) -> Result<BlockStreamEvent<C>, FirehoseError>;

    /// Returns the [BlockPtr] value for this given block number. This is the block pointer
    /// of the longuest according to Firehose view of the blockchain state.
    ///
    /// This is a thin wrapper around [FirehoseEndpoint#block_ptr_for_number] to make
    /// it chain agnostic and callable from chain agnostic [FirehoseBlockStream].
    async fn block_ptr_for_number(
        &self,
        logger: &Logger,
        endpoint: &Arc<FirehoseEndpoint>,
        number: BlockNumber,
    ) -> Result<BlockPtr, Error>;

    /// Returns the closest final block ptr to the block ptr received.
    /// On probablitics chain like Ethereum, final is determined by
    /// the confirmations threshold configured for the Firehose stack (currently
    /// hard-coded to 200).
    ///
    /// On some other chain like NEAR, the actual final block number is determined
    /// from the block itself since it contains information about which block number
    /// is final against the current block.
    ///
    /// To take an example, assuming we are on Ethereum, the final block pointer
    /// for block #10212 would be the determined final block #10012 (10212 - 200 = 10012).
    async fn final_block_ptr_for(
        &self,
        logger: &Logger,
        endpoint: &Arc<FirehoseEndpoint>,
        block: &C::Block,
    ) -> Result<BlockPtr, Error>;
}

#[async_trait]
pub trait BlockStreamMapper<C: Blockchain>: Send + Sync {
    fn decode_block(&self, output: Option<&[u8]>) -> Result<Option<C::Block>, Error>;

    async fn block_with_triggers(
        &self,
        logger: &Logger,
        block: C::Block,
    ) -> Result<BlockWithTriggers<C>, Error>;

    async fn handle_substreams_block(
        &self,
        logger: &Logger,
        clock: Clock,
        cursor: FirehoseCursor,
        block: Vec<u8>,
    ) -> Result<BlockStreamEvent<C>, Error>;

    async fn to_block_stream_event(
        &self,
        logger: &mut Logger,
        message: Option<Message>,
        log_data: &mut SubstreamsLogData,
    ) -> Result<Option<BlockStreamEvent<C>>, SubstreamsError> {
        match message {
            Some(SubstreamsMessage::Session(session_init)) => {
                info!(
                    &logger,
                    "Received session init";
                    "session" => format!("{:?}", session_init),
                );
                log_data.trace_id = session_init.trace_id;
                return Ok(None);
            }
            Some(SubstreamsMessage::BlockUndoSignal(undo)) => {
                let valid_block = match undo.last_valid_block {
                    Some(clock) => clock,
                    None => return Err(SubstreamsError::InvalidUndoError),
                };
                let valid_ptr = BlockPtr {
                    hash: valid_block.id.trim_start_matches("0x").try_into()?,
                    number: valid_block.number as i32,
                };
                log_data.last_seen_block = valid_block.number;
                return Ok(Some(BlockStreamEvent::Revert(
                    valid_ptr,
                    FirehoseCursor::from(undo.last_valid_cursor.clone()),
                )));
            }

            Some(SubstreamsMessage::BlockScopedData(block_scoped_data)) => {
                let BlockScopedData {
                    output,
                    clock,
                    cursor,
                    final_block_height: _,
                    debug_map_outputs: _,
                    debug_store_outputs: _,
                } = block_scoped_data;

                let module_output = match output {
                    Some(out) => out,
                    None => return Ok(None),
                };

                let clock = match clock {
                    Some(clock) => clock,
                    None => return Err(SubstreamsError::MissingClockError),
                };

                let value = match module_output.map_output {
                    Some(Any { type_url: _, value }) => value,
                    None => return Ok(None),
                };

                log_data.last_seen_block = clock.number;
                let cursor = FirehoseCursor::from(cursor);

                let event = self
                    .handle_substreams_block(&logger, clock, cursor, value)
                    .await?;

                Ok(Some(event))
            }

            Some(SubstreamsMessage::Progress(progress)) => {
                if log_data.last_progress.elapsed() > Duration::from_secs(30) {
                    info!(&logger, "{}", log_data.info_string(&progress); "trace_id" => &log_data.trace_id);
                    debug!(&logger, "{}", log_data.debug_string(&progress); "trace_id" => &log_data.trace_id);
                    trace!(
                        &logger,
                        "Received progress update";
                        "progress" => format!("{:?}", progress),
                        "trace_id" => &log_data.trace_id,
                    );
                    log_data.last_progress = Instant::now();
                }
                Ok(None)
            }

            // ignoring Progress messages and SessionInit
            // We are only interested in Data and Undo signals
            _ => Ok(None),
        }
    }
}

#[derive(Error, Debug)]
pub enum FirehoseError {
    /// We were unable to decode the received block payload into the chain specific Block struct (e.g. chain_ethereum::pb::Block)
    #[error("received gRPC block payload cannot be decoded: {0}")]
    DecodingError(#[from] prost::DecodeError),

    /// Some unknown error occurred
    #[error("unknown error")]
    UnknownError(#[from] anyhow::Error),
}

#[derive(Error, Debug)]
pub enum SubstreamsError {
    #[error("response is missing the clock information")]
    MissingClockError,

    #[error("invalid undo message")]
    InvalidUndoError,

    /// We were unable to decode the received block payload into the chain specific Block struct (e.g. chain_ethereum::pb::Block)
    #[error("received gRPC block payload cannot be decoded: {0}")]
    DecodingError(#[from] prost::DecodeError),

    /// Some unknown error occurred
    #[error("unknown error")]
    UnknownError(#[from] anyhow::Error),

    #[error("multiple module output error")]
    MultipleModuleOutputError,

    #[error("module output was not available (none) or wrong data provided")]
    ModuleOutputNotPresentOrUnexpected,

    #[error("unexpected store delta output")]
    UnexpectedStoreDeltaOutput,
}

#[derive(Debug)]
pub enum BlockStreamEvent<C: Blockchain> {
    // The payload is the block the subgraph should revert to, so it becomes the new subgraph head.
    Revert(BlockPtr, FirehoseCursor),

    ProcessBlock(BlockWithTriggers<C>, FirehoseCursor),
    ProcessWasmBlock(BlockPtr, Box<[u8]>, String, FirehoseCursor),
}

impl<C: Blockchain> Clone for BlockStreamEvent<C>
where
    C::TriggerData: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Revert(arg0, arg1) => Self::Revert(arg0.clone(), arg1.clone()),
            Self::ProcessBlock(arg0, arg1) => Self::ProcessBlock(arg0.clone(), arg1.clone()),
            Self::ProcessWasmBlock(arg0, arg1, arg2, arg3) => {
                Self::ProcessWasmBlock(arg0.clone(), arg1.clone(), arg2.clone(), arg3.clone())
            }
        }
    }
}

#[derive(Clone)]
pub struct BlockStreamMetrics {
    pub deployment_head: Box<Gauge>,
    pub deployment_failed: Box<Gauge>,
    pub reverted_blocks: Gauge,
    pub stopwatch: StopwatchMetrics,
}

impl BlockStreamMetrics {
    pub fn new(
        registry: Arc<MetricsRegistry>,
        deployment_id: &DeploymentHash,
        network: String,
        shard: String,
        stopwatch: StopwatchMetrics,
    ) -> Self {
        let reverted_blocks = registry
            .new_deployment_gauge(
                "deployment_reverted_blocks",
                "Track the last reverted block for a subgraph deployment",
                deployment_id.as_str(),
            )
            .expect("Failed to create `deployment_reverted_blocks` gauge");
        let labels = labels! {
            String::from("deployment") => deployment_id.to_string(),
            String::from("network") => network,
            String::from("shard") => shard
        };
        let deployment_head = registry
            .new_gauge(
                "deployment_head",
                "Track the head block number for a deployment",
                labels.clone(),
            )
            .expect("failed to create `deployment_head` gauge");
        let deployment_failed = registry
            .new_gauge(
                "deployment_failed",
                "Boolean gauge to indicate whether the deployment has failed (1 == failed)",
                labels,
            )
            .expect("failed to create `deployment_failed` gauge");
        Self {
            deployment_head,
            deployment_failed,
            reverted_blocks,
            stopwatch,
        }
    }
}

/// Notifications about the chain head advancing. The block ingestor sends
/// an update on this stream whenever the head of the underlying chain
/// changes. The updates have no payload, receivers should call
/// `Store::chain_head_ptr` to check what the latest block is.
pub type ChainHeadUpdateStream = Box<dyn Stream<Item = ()> + Send + Unpin>;

pub trait ChainHeadUpdateListener: Send + Sync + 'static {
    /// Subscribe to chain head updates for the given network.
    fn subscribe(&self, network: String, logger: Logger) -> ChainHeadUpdateStream;
}

#[cfg(test)]
mod test {
    use std::{collections::HashSet, task::Poll};

    use anyhow::Error;
    use futures03::{Stream, StreamExt, TryStreamExt};

    use crate::{
        blockchain::mock::{MockBlock, MockBlockchain},
        ext::futures::{CancelableError, SharedCancelGuard, StreamExtension},
    };

    use super::{
        BlockStream, BlockStreamEvent, BlockWithTriggers, BufferedBlockStream, FirehoseCursor,
    };

    #[derive(Debug)]
    struct TestStream {
        number: u64,
    }

    impl BlockStream<MockBlockchain> for TestStream {
        fn buffer_size_hint(&self) -> usize {
            1
        }
    }

    impl Stream for TestStream {
        type Item = Result<BlockStreamEvent<MockBlockchain>, Error>;

        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            self.number += 1;
            Poll::Ready(Some(Ok(BlockStreamEvent::ProcessBlock(
                BlockWithTriggers::<MockBlockchain> {
                    block: MockBlock {
                        number: self.number - 1,
                    },
                    trigger_data: vec![],
                },
                FirehoseCursor::None,
            ))))
        }
    }

    #[tokio::test]
    async fn consume_stream() {
        let initial_block = 100;
        let buffer_size = 5;

        let stream = Box::new(TestStream {
            number: initial_block,
        });
        let guard = SharedCancelGuard::new();

        let mut stream = BufferedBlockStream::spawn_from_stream(buffer_size, stream)
            .map_err(CancelableError::Error)
            .cancelable(&guard, || Err(CancelableError::Cancel));

        let mut blocks = HashSet::<MockBlock>::new();
        let mut count = 0;
        loop {
            match stream.next().await {
                None if blocks.is_empty() => panic!("None before blocks"),
                Some(Err(CancelableError::Cancel)) => {
                    assert!(guard.is_canceled(), "Guard shouldn't be called yet");

                    break;
                }
                Some(Ok(BlockStreamEvent::ProcessBlock(block_triggers, _))) => {
                    let block = block_triggers.block;
                    blocks.insert(block.clone());
                    count += 1;

                    if block.number > initial_block + buffer_size as u64 {
                        guard.cancel();
                    }
                }
                _ => panic!("Should not happen"),
            };
        }
        assert!(
            blocks.len() > buffer_size,
            "should consume at least a full buffer, consumed {}",
            count
        );
        assert_eq!(count, blocks.len(), "should not have duplicated blocks");
    }
}
