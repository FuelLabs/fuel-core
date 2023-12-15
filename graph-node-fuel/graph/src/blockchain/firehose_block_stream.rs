use super::block_stream::{
    BlockStream, BlockStreamEvent, FirehoseMapper, FIREHOSE_BUFFER_STREAM_SIZE,
};
use super::client::ChainClient;
use super::Blockchain;
use crate::blockchain::block_stream::FirehoseCursor;
use crate::blockchain::TriggerFilter;
use crate::prelude::*;
use crate::util::backoff::ExponentialBackoff;
use crate::{firehose, firehose::FirehoseEndpoint};
use async_stream::try_stream;
use futures03::{Stream, StreamExt};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tonic::Status;

struct FirehoseBlockStreamMetrics {
    deployment: DeploymentHash,
    restarts: CounterVec,
    connect_duration: GaugeVec,
    time_between_responses: HistogramVec,
    responses: CounterVec,
}

impl FirehoseBlockStreamMetrics {
    pub fn new(registry: Arc<MetricsRegistry>, deployment: DeploymentHash) -> Self {
        Self {
            deployment,

            restarts: registry
                .global_counter_vec(
                    "deployment_firehose_blockstream_restarts",
                    "Counts the number of times a Firehose block stream is (re)started",
                    vec!["deployment", "provider", "success"].as_slice(),
                )
                .unwrap(),

            connect_duration: registry
                .global_gauge_vec(
                    "deployment_firehose_blockstream_connect_duration",
                    "Measures the time it takes to connect a Firehose block stream",
                    vec!["deployment", "provider"].as_slice(),
                )
                .unwrap(),

            time_between_responses: registry
                .global_histogram_vec(
                    "deployment_firehose_blockstream_time_between_responses",
                    "Measures the time between receiving and processing Firehose stream responses",
                    vec!["deployment", "provider"].as_slice(),
                )
                .unwrap(),

            responses: registry
                .global_counter_vec(
                    "deployment_firehose_blockstream_responses",
                    "Counts the number of responses received from a Firehose block stream",
                    vec!["deployment", "provider", "kind"].as_slice(),
                )
                .unwrap(),
        }
    }

    fn observe_successful_connection(&self, time: &mut Instant, provider: &str) {
        self.restarts
            .with_label_values(&[&self.deployment, &provider, "true"])
            .inc();
        self.connect_duration
            .with_label_values(&[&self.deployment, &provider])
            .set(time.elapsed().as_secs_f64());

        // Reset last connection timestamp
        *time = Instant::now();
    }

    fn observe_failed_connection(&self, time: &mut Instant, provider: &str) {
        self.restarts
            .with_label_values(&[&self.deployment, &provider, "false"])
            .inc();
        self.connect_duration
            .with_label_values(&[&self.deployment, &provider])
            .set(time.elapsed().as_secs_f64());

        // Reset last connection timestamp
        *time = Instant::now();
    }

    fn observe_response(&self, kind: &str, time: &mut Instant, provider: &str) {
        self.time_between_responses
            .with_label_values(&[&self.deployment, &provider])
            .observe(time.elapsed().as_secs_f64());
        self.responses
            .with_label_values(&[&self.deployment, &provider, kind])
            .inc();

        // Reset last response timestamp
        *time = Instant::now();
    }
}

pub struct FirehoseBlockStream<C: Blockchain> {
    stream: Pin<Box<dyn Stream<Item = Result<BlockStreamEvent<C>, Error>> + Send>>,
}

impl<C> FirehoseBlockStream<C>
where
    C: Blockchain,
{
    pub fn new<F>(
        deployment: DeploymentHash,
        client: Arc<ChainClient<C>>,
        subgraph_current_block: Option<BlockPtr>,
        cursor: FirehoseCursor,
        mapper: Arc<F>,
        start_blocks: Vec<BlockNumber>,
        logger: Logger,
        registry: Arc<MetricsRegistry>,
    ) -> Self
    where
        F: FirehoseMapper<C> + 'static,
    {
        if !client.is_firehose() {
            unreachable!("Firehose block stream called with rpc endpoint");
        }

        let manifest_start_block_num = start_blocks
            .into_iter()
            .min()
            // Firehose knows where to start the stream for the specific chain, 0 here means
            // start at Genesis block.
            .unwrap_or(0);

        let metrics = FirehoseBlockStreamMetrics::new(registry, deployment.clone());
        FirehoseBlockStream {
            stream: Box::pin(stream_blocks(
                client,
                cursor,
                deployment,
                mapper,
                manifest_start_block_num,
                subgraph_current_block,
                logger,
                metrics,
            )),
        }
    }
}

fn stream_blocks<C: Blockchain, F: FirehoseMapper<C>>(
    client: Arc<ChainClient<C>>,
    mut latest_cursor: FirehoseCursor,
    deployment: DeploymentHash,
    mapper: Arc<F>,
    manifest_start_block_num: BlockNumber,
    subgraph_current_block: Option<BlockPtr>,
    logger: Logger,
    metrics: FirehoseBlockStreamMetrics,
) -> impl Stream<Item = Result<BlockStreamEvent<C>, Error>> {
    let mut subgraph_current_block = subgraph_current_block;
    let mut start_block_num = subgraph_current_block
        .as_ref()
        .map(|ptr| {
            // Firehose start block is inclusive while the subgraph_current_block is where the actual
            // subgraph is currently at. So to process the actual next block, we must start one block
            // further in the chain.
            ptr.block_number() + 1 as BlockNumber
        })
        .unwrap_or(manifest_start_block_num);

    // Sanity check when starting from a subgraph block ptr directly. When
    // this happens, we must ensure that Firehose first picked block directly follows the
    // subgraph block ptr. So we check that Firehose first picked block's parent is
    // equal to subgraph block ptr.
    //
    // This can happen for example when rewinding, unfailing a deterministic error or
    // when switching from RPC to Firehose on Ethereum.
    //
    // What could go wrong is that the subgraph block ptr points to a forked block but
    // since Firehose only accepts `block_number`, it could pick right away the canonical
    // block of the longuest chain creating inconsistencies in the data (because it would
    // not revert the forked the block).
    //
    // If a Firehose cursor is present, it's used to resume the stream and as such, there is no need to
    // perform the chain continuity check.
    //
    // If there was no cursor, now we need to check if the subgraph current block is set to something.
    // When the graph node deploys a new subgraph, it always create a subgraph ptr for this subgraph, the
    // initial subgraph block pointer points to the parent block of the manifest's start block, which is usually
    // equivalent (but not always) to manifest's start block number - 1.
    //
    // Hence, we only need to check the chain continuity if the subgraph current block ptr is higher or equal
    // to the subgraph manifest's start block number. Indeed, only in this case (and when there is no firehose
    // cursor) it means the subgraph was started and advanced with something else than Firehose and as such,
    // chain continuity check needs to be performed.
    let mut check_subgraph_continuity = must_check_subgraph_continuity(
        &logger,
        &subgraph_current_block,
        &latest_cursor,
        manifest_start_block_num,
    );
    if check_subgraph_continuity {
        debug!(&logger, "Going to check continuity of chain on first block");
    }

    // Back off exponentially whenever we encounter a connection error or a stream with bad data
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(500), Duration::from_secs(45));

    // This attribute is needed because `try_stream!` seems to break detection of `skip_backoff` assignments
    #[allow(unused_assignments)]
    let mut skip_backoff = false;

    try_stream! {
        loop {
            let endpoint = client.firehose_endpoint()?;
            let logger = logger.new(o!("deployment" => deployment.clone(), "provider" => endpoint.provider.to_string()));

            info!(
                &logger,
                "Blockstream disconnected, connecting";
                "endpoint_uri" => format_args!("{}", endpoint),
                "start_block" => start_block_num,
                "subgraph" => &deployment,
                "cursor" => latest_cursor.to_string(),
                "provider_err_count" => endpoint.current_error_count(),
            );

            // We just reconnected, assume that we want to back off on errors
            skip_backoff = false;

            let mut request = firehose::Request {
                start_block_num: start_block_num as i64,
                cursor: latest_cursor.to_string(),
                final_blocks_only: false,
                ..Default::default()
            };

            if endpoint.filters_enabled {
                request.transforms = mapper.trigger_filter().clone().to_firehose_filter();
            }

            let mut connect_start = Instant::now();
            let req = endpoint.clone().stream_blocks(request);
            let result = tokio::time::timeout(Duration::from_secs(120), req).await.map_err(|x| x.into()).and_then(|x| x);

            match result {
                Ok(stream) => {
                    info!(&logger, "Blockstream connected");

                    // Track the time it takes to set up the block stream
                    metrics.observe_successful_connection(&mut connect_start, &endpoint.provider);

                    let mut last_response_time = Instant::now();
                    let mut expected_stream_end = false;

                    for await response in stream {
                        match process_firehose_response(
                            &endpoint,
                            response,
                            &mut check_subgraph_continuity,
                            manifest_start_block_num,
                            subgraph_current_block.as_ref(),
                            mapper.as_ref(),
                            &logger,
                        ).await {
                            Ok(BlockResponse::Proceed(event, cursor)) => {
                                // Reset backoff because we got a good value from the stream
                                backoff.reset();

                                metrics.observe_response("proceed", &mut last_response_time, &endpoint.provider);

                                yield event;

                                latest_cursor = FirehoseCursor::from(cursor);
                            },
                            Ok(BlockResponse::Rewind(revert_to)) => {
                                // Reset backoff because we got a good value from the stream
                                backoff.reset();

                                metrics.observe_response("rewind", &mut last_response_time, &endpoint.provider);

                                // It's totally correct to pass the None as the cursor here, if we are here, there
                                // was no cursor before anyway, so it's totally fine to pass `None`
                                yield BlockStreamEvent::Revert(revert_to.clone(), FirehoseCursor::None);

                                latest_cursor = FirehoseCursor::None;

                                // We have to reconnect (see below) but we don't wait to wait before doing
                                // that, so skip the optional backing off at the end of the loop
                                skip_backoff = true;

                                // We must restart the stream to ensure we now send block from revert_to point
                                // and we add + 1 to start block num because Firehose is inclusive and as such,
                                // we need to move to "next" block.
                                start_block_num = revert_to.number + 1;
                                subgraph_current_block = Some(revert_to);
                                expected_stream_end = true;
                                break;
                            },
                            Err(err) => {
                                // We have an open connection but there was an error processing the Firehose
                                // response. We will reconnect the stream after this; this is the case where
                                // we actually _want_ to back off in case we keep running into the same error.
                                // An example of this situation is if we get invalid block or transaction data
                                // that cannot be decoded properly.

                                metrics.observe_response("error", &mut last_response_time, &endpoint.provider);

                                error!(logger, "{:#}", err);
                                expected_stream_end = true;
                                break;
                            }
                        }
                    }

                    if !expected_stream_end {
                        error!(logger, "Stream blocks complete unexpectedly, expecting stream to always stream blocks");
                    }
                },
                Err(e) => {
                    // We failed to connect and will try again; this is another
                    // case where we actually _want_ to back off in case we keep
                    // having connection errors.

                    metrics.observe_failed_connection(&mut connect_start, &endpoint.provider);

                    error!(logger, "Unable to connect to endpoint: {:#}", e);
                }
            }

            // If we reach this point, we must wait a bit before retrying, unless `skip_backoff` is true
            if !skip_backoff {
                backoff.sleep_async().await;
            }
        }
    }
}

enum BlockResponse<C: Blockchain> {
    Proceed(BlockStreamEvent<C>, String),
    Rewind(BlockPtr),
}

async fn process_firehose_response<C: Blockchain, F: FirehoseMapper<C>>(
    endpoint: &Arc<FirehoseEndpoint>,
    result: Result<firehose::Response, Status>,
    check_subgraph_continuity: &mut bool,
    manifest_start_block_num: BlockNumber,
    subgraph_current_block: Option<&BlockPtr>,
    mapper: &F,
    logger: &Logger,
) -> Result<BlockResponse<C>, Error> {
    let response = result.context("An error occurred while streaming blocks")?;

    let event = mapper
        .to_block_stream_event(logger, &response)
        .await
        .context("Mapping block to BlockStreamEvent failed")?;

    if *check_subgraph_continuity {
        info!(logger, "Firehose started from a subgraph pointer without an existing cursor, ensuring chain continuity");

        if let BlockStreamEvent::ProcessBlock(ref block, _) = event {
            let previous_block_ptr = block.parent_ptr();
            if previous_block_ptr.is_some() && previous_block_ptr.as_ref() != subgraph_current_block
            {
                warn!(&logger,
                    "Firehose selected first streamed block's parent should match subgraph start block, reverting to last know final chain segment";
                    "subgraph_current_block" => &subgraph_current_block.unwrap(),
                    "firehose_start_block" => &previous_block_ptr.unwrap(),
                );

                let mut revert_to = mapper
                    .final_block_ptr_for(logger, endpoint, &block.block)
                    .await
                    .context("Could not fetch final block to revert to")?;

                if revert_to.number < manifest_start_block_num {
                    warn!(&logger, "We would return before subgraph manifest's start block, limiting rewind to manifest's start block");

                    // We must revert up to parent's of manifest start block to ensure we delete everything "including" the start
                    // block that was processed.
                    let mut block_num = manifest_start_block_num - 1;
                    if block_num < 0 {
                        block_num = 0;
                    }

                    revert_to = mapper
                        .block_ptr_for_number(logger, endpoint, block_num)
                        .await
                        .context("Could not fetch manifest start block to revert to")?;
                }

                return Ok(BlockResponse::Rewind(revert_to));
            }
        }

        info!(
            logger,
            "Subgraph chain continuity is respected, proceeding normally"
        );
        *check_subgraph_continuity = false;
    }

    Ok(BlockResponse::Proceed(event, response.cursor))
}

impl<C: Blockchain> Stream for FirehoseBlockStream<C> {
    type Item = Result<BlockStreamEvent<C>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

impl<C: Blockchain> BlockStream<C> for FirehoseBlockStream<C> {
    fn buffer_size_hint(&self) -> usize {
        FIREHOSE_BUFFER_STREAM_SIZE
    }
}

fn must_check_subgraph_continuity(
    logger: &Logger,
    subgraph_current_block: &Option<BlockPtr>,
    subgraph_cursor: &FirehoseCursor,
    subgraph_manifest_start_block_number: i32,
) -> bool {
    match subgraph_current_block {
        Some(current_block) if subgraph_cursor.is_none() => {
            debug!(&logger, "Checking if subgraph current block is after manifest start block";
                "subgraph_current_block_number" => current_block.number,
                "manifest_start_block_number" => subgraph_manifest_start_block_number,
            );

            current_block.number >= subgraph_manifest_start_block_number
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::blockchain::{
        block_stream::FirehoseCursor, firehose_block_stream::must_check_subgraph_continuity,
        BlockPtr,
    };
    use slog::{o, Logger};

    #[test]
    fn check_continuity() {
        let logger = Logger::root(slog::Discard, o!());
        let no_current_block: Option<BlockPtr> = None;
        let no_cursor = FirehoseCursor::None;
        let some_cursor = FirehoseCursor::from("abc".to_string());
        let some_current_block = |number: i32| -> Option<BlockPtr> {
            Some(BlockPtr {
                hash: vec![0xab, 0xcd].into(),
                number,
            })
        };

        // Nothing

        assert_eq!(
            must_check_subgraph_continuity(&logger, &no_current_block, &no_cursor, 10),
            false,
        );

        // No cursor, subgraph current block ptr <, ==, > than manifest start block num

        assert_eq!(
            must_check_subgraph_continuity(&logger, &some_current_block(9), &no_cursor, 10),
            false,
        );

        assert_eq!(
            must_check_subgraph_continuity(&logger, &some_current_block(10), &no_cursor, 10),
            true,
        );

        assert_eq!(
            must_check_subgraph_continuity(&logger, &some_current_block(11), &no_cursor, 10),
            true,
        );

        // Some cursor, subgraph current block ptr <, ==, > than manifest start block num

        assert_eq!(
            must_check_subgraph_continuity(&logger, &no_current_block, &some_cursor, 10),
            false,
        );

        assert_eq!(
            must_check_subgraph_continuity(&logger, &some_current_block(9), &some_cursor, 10),
            false,
        );

        assert_eq!(
            must_check_subgraph_continuity(&logger, &some_current_block(10), &some_cursor, 10),
            false,
        );

        assert_eq!(
            must_check_subgraph_continuity(&logger, &some_current_block(11), &some_cursor, 10),
            false,
        );
    }
}
