use super::block_stream::{BlockStreamMapper, SUBSTREAMS_BUFFER_STREAM_SIZE};
use super::client::ChainClient;
use crate::blockchain::block_stream::{BlockStream, BlockStreamEvent};
use crate::blockchain::Blockchain;
use crate::prelude::*;
use crate::substreams::Modules;
use crate::substreams_rpc::{ModulesProgress, Request, Response};
use crate::util::backoff::ExponentialBackoff;
use async_stream::try_stream;
use futures03::{Stream, StreamExt};
use humantime::format_duration;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tonic::Status;

struct SubstreamsBlockStreamMetrics {
    deployment: DeploymentHash,
    restarts: CounterVec,
    connect_duration: GaugeVec,
    time_between_responses: HistogramVec,
    responses: CounterVec,
}

impl SubstreamsBlockStreamMetrics {
    pub fn new(registry: Arc<MetricsRegistry>, deployment: DeploymentHash) -> Self {
        Self {
            deployment,
            restarts: registry
                .global_counter_vec(
                    "deployment_substreams_blockstream_restarts",
                    "Counts the number of times a Substreams block stream is (re)started",
                    vec!["deployment", "provider", "success"].as_slice(),
                )
                .unwrap(),

            connect_duration: registry
                .global_gauge_vec(
                    "deployment_substreams_blockstream_connect_duration",
                    "Measures the time it takes to connect a Substreams block stream",
                    vec!["deployment", "provider"].as_slice(),
                )
                .unwrap(),

            time_between_responses: registry
                .global_histogram_vec(
                    "deployment_substreams_blockstream_time_between_responses",
                    "Measures the time between receiving and processing Substreams stream responses",
                    vec!["deployment", "provider"].as_slice(),
                )
                .unwrap(),

            responses: registry
                .global_counter_vec(
                    "deployment_substreams_blockstream_responses",
                    "Counts the number of responses received from a Substreams block stream",
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

pub struct SubstreamsBlockStream<C: Blockchain> {
    //fixme: not sure if this is ok to be set as public, maybe
    // we do not want to expose the stream to the caller
    stream: Pin<Box<dyn Stream<Item = Result<BlockStreamEvent<C>, Error>> + Send>>,
}

impl<C> SubstreamsBlockStream<C>
where
    C: Blockchain,
{
    pub fn new<F>(
        deployment: DeploymentHash,
        client: Arc<ChainClient<C>>,
        subgraph_current_block: Option<BlockPtr>,
        cursor: Option<String>,
        mapper: Arc<F>,
        modules: Option<Modules>,
        module_name: String,
        start_blocks: Vec<BlockNumber>,
        end_blocks: Vec<BlockNumber>,
        logger: Logger,
        registry: Arc<MetricsRegistry>,
    ) -> Self
    where
        F: BlockStreamMapper<C> + 'static,
    {
        let manifest_start_block_num = start_blocks.into_iter().min().unwrap_or(0);

        let manifest_end_block_num = end_blocks.into_iter().min().unwrap_or(0);

        let metrics = SubstreamsBlockStreamMetrics::new(registry, deployment.clone());

        SubstreamsBlockStream {
            stream: Box::pin(stream_blocks(
                client,
                cursor,
                deployment,
                mapper,
                modules,
                module_name,
                manifest_start_block_num,
                manifest_end_block_num,
                subgraph_current_block,
                logger,
                metrics,
            )),
        }
    }
}

fn stream_blocks<C: Blockchain, F: BlockStreamMapper<C>>(
    client: Arc<ChainClient<C>>,
    cursor: Option<String>,
    deployment: DeploymentHash,
    mapper: Arc<F>,
    modules: Option<Modules>,
    module_name: String,
    manifest_start_block_num: BlockNumber,
    manifest_end_block_num: BlockNumber,
    subgraph_current_block: Option<BlockPtr>,
    logger: Logger,
    metrics: SubstreamsBlockStreamMetrics,
) -> impl Stream<Item = Result<BlockStreamEvent<C>, Error>> {
    let mut latest_cursor = cursor.unwrap_or_default();

    let start_block_num = subgraph_current_block
        .as_ref()
        .map(|ptr| {
            // current_block has already been processed, we start at next block
            ptr.block_number() as i64 + 1
        })
        .unwrap_or(manifest_start_block_num as i64);

    let stop_block_num = manifest_end_block_num as u64;

    // Back off exponentially whenever we encounter a connection error or a stream with bad data
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(500), Duration::from_secs(45));

    // This attribute is needed because `try_stream!` seems to break detection of `skip_backoff` assignments
    #[allow(unused_assignments)]
    let mut skip_backoff = false;

    let mut log_data = SubstreamsLogData::new();

    try_stream! {
            let endpoint = client.firehose_endpoint()?;
            let mut logger = logger.new(o!("deployment" => deployment.clone(), "provider" => endpoint.provider.to_string()));

        loop {
            info!(
                &logger,
                "Blockstreams disconnected, connecting";
                "endpoint_uri" => format_args!("{}", endpoint),
                "subgraph" => &deployment,
                "start_block" => start_block_num,
                "cursor" => &latest_cursor,
                "provider_err_count" => endpoint.current_error_count(),
            );

            // We just reconnected, assume that we want to back off on errors
            skip_backoff = false;

            let mut connect_start = Instant::now();
            let request = Request {
                start_block_num,
                start_cursor: latest_cursor.clone(),
                stop_block_num,
                modules: modules.clone(),
                output_module: module_name.clone(),
                production_mode: true,
                ..Default::default()
            };


            let result = endpoint.clone().substreams(request).await;

            match result {
                Ok(stream) => {
                    info!(&logger, "Blockstreams connected");

                    // Track the time it takes to set up the block stream
                    metrics.observe_successful_connection(&mut connect_start, &endpoint.provider);

                    let mut last_response_time = Instant::now();
                    let mut expected_stream_end = false;

                    for await response in stream{
                        match process_substreams_response(
                            response,
                            mapper.as_ref(),
                            &mut logger,
                            &mut log_data,
                        ).await {
                            Ok(block_response) => {
                                match block_response {
                                    None => {}
                                    Some(BlockResponse::Proceed(event, cursor)) => {
                                        // Reset backoff because we got a good value from the stream
                                        backoff.reset();

                                        metrics.observe_response("proceed", &mut last_response_time, &endpoint.provider);

                                        yield event;

                                        latest_cursor = cursor;
                                    }
                                }
                            },
                            Err(err) => {
                                info!(&logger, "received err");
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
}

async fn process_substreams_response<C: Blockchain, F: BlockStreamMapper<C>>(
    result: Result<Response, Status>,
    mapper: &F,
    logger: &mut Logger,
    log_data: &mut SubstreamsLogData,
) -> Result<Option<BlockResponse<C>>, Error> {
    let response = match result {
        Ok(v) => v,
        Err(e) => return Err(anyhow!("An error occurred while streaming blocks: {:#}", e)),
    };

    match mapper
        .to_block_stream_event(logger, response.message, log_data)
        .await
        .context("Mapping message to BlockStreamEvent failed")?
    {
        Some(event) => {
            let cursor = match &event {
                BlockStreamEvent::Revert(_, cursor) => cursor,
                BlockStreamEvent::ProcessBlock(_, cursor) => cursor,
                BlockStreamEvent::ProcessWasmBlock(_, _, _, cursor) => cursor,
            }
            .to_string();

            return Ok(Some(BlockResponse::Proceed(event, cursor)));
        }
        None => Ok(None), // some progress responses are ignored within to_block_stream_event
    }
}

impl<C: Blockchain> Stream for SubstreamsBlockStream<C> {
    type Item = Result<BlockStreamEvent<C>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

impl<C: Blockchain> BlockStream<C> for SubstreamsBlockStream<C> {
    fn buffer_size_hint(&self) -> usize {
        SUBSTREAMS_BUFFER_STREAM_SIZE
    }
}

pub struct SubstreamsLogData {
    pub last_progress: Instant,
    pub last_seen_block: u64,
    pub trace_id: String,
}

impl SubstreamsLogData {
    fn new() -> SubstreamsLogData {
        SubstreamsLogData {
            last_progress: Instant::now(),
            last_seen_block: 0,
            trace_id: "".to_string(),
        }
    }
    pub fn info_string(&self, progress: &ModulesProgress) -> String {
        format!(
            "Substreams backend graph_out last block is {}, {} stages, {} jobs",
            self.last_seen_block,
            progress.stages.len(),
            progress.running_jobs.len()
        )
    }
    pub fn debug_string(&self, progress: &ModulesProgress) -> String {
        let len = progress.stages.len();
        let mut stages_str = "".to_string();
        for i in (0..len).rev() {
            let stage = &progress.stages[i];
            let range = if stage.completed_ranges.len() > 0 {
                let b = stage.completed_ranges.iter().map(|x| x.end_block).min();
                format!(" up to {}", b.unwrap_or(0))
            } else {
                "".to_string()
            };
            let mlen = stage.modules.len();
            let module = if mlen == 0 {
                "".to_string()
            } else if mlen == 1 {
                format!(" ({})", stage.modules[0])
            } else {
                format!(" ({} +{})", stage.modules[mlen - 1], mlen - 1)
            };
            if !stages_str.is_empty() {
                stages_str.push_str(", ");
            }
            stages_str.push_str(&format!("#{}{}{}", i, range, module));
        }
        let stage_str = if len > 0 {
            format!(" Stages: [{}]", stages_str)
        } else {
            "".to_string()
        };
        let mut jobs_str = "".to_string();
        let jlen = progress.running_jobs.len();
        for i in 0..jlen {
            let job = &progress.running_jobs[i];
            if !jobs_str.is_empty() {
                jobs_str.push_str(", ");
            }
            let duration_str = format_duration(Duration::from_millis(job.duration_ms));
            jobs_str.push_str(&format!(
                "#{} on Stage {} @ {} | +{}|{} elapsed {}",
                i,
                job.stage,
                job.start_block,
                job.processed_blocks,
                job.stop_block - job.start_block,
                duration_str
            ));
        }
        let job_str = if jlen > 0 {
            format!(", Jobs: [{}]", jobs_str)
        } else {
            "".to_string()
        };
        format!(
            "Substreams backend graph_out last block is {},{}{}",
            self.last_seen_block, stage_str, job_str,
        )
    }
}
