use futures::future;
use futures::prelude::*;
use futures03::{future::BoxFuture, stream::FuturesUnordered};
use graph::blockchain::client::ChainClient;
use graph::blockchain::BlockHash;
use graph::blockchain::ChainIdentifier;
use graph::components::transaction_receipt::LightTransactionReceipt;
use graph::data::subgraph::UnifiedMappingApiVersion;
use graph::data::subgraph::API_VERSION_0_0_7;
use graph::prelude::ethabi::ParamType;
use graph::prelude::ethabi::Token;
use graph::prelude::tokio::try_join;
use graph::slog::o;
use graph::{
    blockchain::{block_stream::BlockWithTriggers, BlockPtr, IngestorError},
    prelude::{
        anyhow::{self, anyhow, bail, ensure, Context},
        async_trait, debug, error, ethabi,
        futures03::{self, compat::Future01CompatExt, FutureExt, StreamExt, TryStreamExt},
        hex, info, retry, serde_json as json, stream, tiny_keccak, trace, warn,
        web3::{
            self,
            types::{
                Address, BlockId, BlockNumber as Web3BlockNumber, Bytes, CallRequest, Filter,
                FilterBuilder, Log, Transaction, TransactionReceipt, H256,
            },
        },
        BlockNumber, ChainStore, CheapClone, DynTryFuture, Error, EthereumCallCache, Logger,
        TimeoutError, TryFutureExt,
    },
};
use graph::{
    components::ethereum::*,
    prelude::web3::api::Web3,
    prelude::web3::transports::Batch,
    prelude::web3::types::{Trace, TraceFilter, TraceFilterBuilder, H160},
};
use itertools::Itertools;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::convert::TryFrom;
use std::iter::FromIterator;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::adapter::ProviderStatus;
use crate::chain::BlockFinality;
use crate::trigger::LogRef;
use crate::Chain;
use crate::NodeCapabilities;
use crate::{
    adapter::{
        EthGetLogsFilter, EthereumAdapter as EthereumAdapterTrait, EthereumBlockFilter,
        EthereumCallFilter, EthereumContractCall, EthereumContractCallError, EthereumLogFilter,
        ProviderEthRpcMetrics, SubgraphEthRpcMetrics,
    },
    transport::Transport,
    trigger::{EthereumBlockTriggerType, EthereumTrigger},
    TriggerFilter, ENV_VARS,
};

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    logger: Logger,
    provider: String,
    web3: Arc<Web3<Transport>>,
    metrics: Arc<ProviderEthRpcMetrics>,
    supports_eip_1898: bool,
    call_only: bool,
}

impl CheapClone for EthereumAdapter {
    fn cheap_clone(&self) -> Self {
        Self {
            logger: self.logger.clone(),
            provider: self.provider.clone(),
            web3: self.web3.cheap_clone(),
            metrics: self.metrics.cheap_clone(),
            supports_eip_1898: self.supports_eip_1898,
            call_only: self.call_only,
        }
    }
}

impl EthereumAdapter {
    pub fn is_call_only(&self) -> bool {
        self.call_only
    }

    pub async fn new(
        logger: Logger,
        provider: String,
        transport: Transport,
        provider_metrics: Arc<ProviderEthRpcMetrics>,
        supports_eip_1898: bool,
        call_only: bool,
    ) -> Self {
        let web3 = Arc::new(Web3::new(transport));

        // Use the client version to check if it is ganache. For compatibility with unit tests, be
        // are lenient with errors, defaulting to false.
        let is_ganache = web3
            .web3()
            .client_version()
            .await
            .map(|s| s.contains("TestRPC"))
            .unwrap_or(false);

        EthereumAdapter {
            logger,
            provider,
            web3,
            metrics: provider_metrics,
            supports_eip_1898: supports_eip_1898 && !is_ganache,
            call_only,
        }
    }

    async fn traces(
        self,
        logger: Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: BlockNumber,
        to: BlockNumber,
        addresses: Vec<H160>,
    ) -> Result<Vec<Trace>, Error> {
        assert!(!self.call_only);

        let eth = self.clone();
        let retry_log_message =
            format!("trace_filter RPC call for block range: [{}..{}]", from, to);
        retry(retry_log_message, &logger)
            .limit(ENV_VARS.request_retries)
            .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
            .run(move || {
                let trace_filter: TraceFilter = match addresses.len() {
                    0 => TraceFilterBuilder::default()
                        .from_block(from.into())
                        .to_block(to.into())
                        .build(),
                    _ => TraceFilterBuilder::default()
                        .from_block(from.into())
                        .to_block(to.into())
                        .to_address(addresses.clone())
                        .build(),
                };

                let eth = eth.cheap_clone();
                let logger_for_triggers = logger.clone();
                let logger_for_error = logger.clone();
                let start = Instant::now();
                let subgraph_metrics = subgraph_metrics.clone();
                let provider_metrics = eth.metrics.clone();
                let provider = self.provider.clone();

                async move {
                    let result = eth
                        .web3
                        .trace()
                        .filter(trace_filter)
                        .await
                        .map(move |traces| {
                            if !traces.is_empty() {
                                if to == from {
                                    debug!(
                                        logger_for_triggers,
                                        "Received {} traces for block {}",
                                        traces.len(),
                                        to
                                    );
                                } else {
                                    debug!(
                                        logger_for_triggers,
                                        "Received {} traces for blocks [{}, {}]",
                                        traces.len(),
                                        from,
                                        to
                                    );
                                }
                            }
                            traces
                        })
                        .map_err(Error::from);

                    let elapsed = start.elapsed().as_secs_f64();
                    provider_metrics.observe_request(elapsed, "trace_filter", &provider);
                    subgraph_metrics.observe_request(elapsed, "trace_filter", &provider);
                    if let Err(e) = &result {
                        provider_metrics.add_error("trace_filter", &provider);
                        subgraph_metrics.add_error("trace_filter", &provider);
                        debug!(
                            logger_for_error,
                            "Error querying traces error = {:#} from = {} to = {}", e, from, to
                        );
                    }
                    result
                }
            })
            .map_err(move |e| {
                e.into_inner().unwrap_or_else(move || {
                    anyhow::anyhow!(
                        "Ethereum node took too long to respond to trace_filter \
                         (from block {}, to block {})",
                        from,
                        to
                    )
                })
            })
            .await
    }

    async fn logs_with_sigs(
        &self,
        logger: Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: BlockNumber,
        to: BlockNumber,
        filter: Arc<EthGetLogsFilter>,
        too_many_logs_fingerprints: &'static [&'static str],
    ) -> Result<Vec<Log>, TimeoutError<web3::error::Error>> {
        assert!(!self.call_only);

        let eth_adapter = self.clone();
        let retry_log_message = format!("eth_getLogs RPC call for block range: [{}..{}]", from, to);
        retry(retry_log_message, &logger)
            .when(move |res: &Result<_, web3::error::Error>| match res {
                Ok(_) => false,
                Err(e) => !too_many_logs_fingerprints
                    .iter()
                    .any(|f| e.to_string().contains(f)),
            })
            .limit(ENV_VARS.request_retries)
            .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
            .run(move || {
                let eth_adapter = eth_adapter.cheap_clone();
                let subgraph_metrics = subgraph_metrics.clone();
                let provider_metrics = eth_adapter.metrics.clone();
                let filter = filter.clone();
                let provider = eth_adapter.provider.clone();

                async move {
                    let start = Instant::now();

                    // Create a log filter
                    let log_filter: Filter = FilterBuilder::default()
                        .from_block(from.into())
                        .to_block(to.into())
                        .address(filter.contracts.clone())
                        .topics(Some(filter.event_signatures.clone()), None, None, None)
                        .build();

                    // Request logs from client
                    let result = eth_adapter.web3.eth().logs(log_filter).boxed().await;
                    let elapsed = start.elapsed().as_secs_f64();
                    provider_metrics.observe_request(elapsed, "eth_getLogs", &provider);
                    subgraph_metrics.observe_request(elapsed, "eth_getLogs", &provider);
                    if result.is_err() {
                        provider_metrics.add_error("eth_getLogs", &provider);
                        subgraph_metrics.add_error("eth_getLogs", &provider);
                    }
                    result
                }
            })
            .await
    }

    fn trace_stream(
        self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: BlockNumber,
        to: BlockNumber,
        addresses: Vec<H160>,
    ) -> impl Stream<Item = Trace, Error = Error> + Send {
        if from > to {
            panic!(
                "Can not produce a call stream on a backwards block range: from = {}, to = {}",
                from, to,
            );
        }

        // Go one block at a time if requesting all traces, to not overload the RPC.
        let step_size = match addresses.is_empty() {
            false => ENV_VARS.trace_stream_step_size,
            true => 1,
        };

        let eth = self;
        let logger = logger.clone();
        stream::unfold(from, move |start| {
            if start > to {
                return None;
            }
            let end = (start + step_size - 1).min(to);
            let new_start = end + 1;
            if start == end {
                debug!(logger, "Requesting traces for block {}", start);
            } else {
                debug!(logger, "Requesting traces for blocks [{}, {}]", start, end);
            }
            Some(futures::future::ok((
                eth.clone()
                    .traces(
                        logger.cheap_clone(),
                        subgraph_metrics.clone(),
                        start,
                        end,
                        addresses.clone(),
                    )
                    .boxed()
                    .compat(),
                new_start,
            )))
        })
        .buffered(ENV_VARS.block_batch_size)
        .map(stream::iter_ok)
        .flatten()
    }

    fn log_stream(
        &self,
        logger: Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: BlockNumber,
        to: BlockNumber,
        filter: EthGetLogsFilter,
    ) -> DynTryFuture<'static, Vec<Log>, Error> {
        // Codes returned by Ethereum node providers if an eth_getLogs request is too heavy.
        // The first one is for Infura when it hits the log limit, the rest for Alchemy timeouts.
        const TOO_MANY_LOGS_FINGERPRINTS: &[&str] = &[
            "ServerError(-32005)",
            "503 Service Unavailable",
            "ServerError(-32000)",
        ];

        if from > to {
            panic!(
                "cannot produce a log stream on a backwards block range (from={}, to={})",
                from, to
            );
        }

        // Collect all event sigs
        let eth = self.cheap_clone();
        let filter = Arc::new(filter);

        let step = match filter.contracts.is_empty() {
            // `to - from + 1`  blocks will be scanned.
            false => to - from,
            true => (to - from).min(ENV_VARS.max_event_only_range - 1),
        };

        // Typically this will loop only once and fetch the entire range in one request. But if the
        // node returns an error that signifies the request is to heavy to process, the range will
        // be broken down to smaller steps.
        futures03::stream::try_unfold((from, step), move |(start, step)| {
            let logger = logger.cheap_clone();
            let filter = filter.cheap_clone();
            let eth = eth.cheap_clone();
            let subgraph_metrics = subgraph_metrics.cheap_clone();

            async move {
                if start > to {
                    return Ok(None);
                }

                let end = (start + step).min(to);
                debug!(
                    logger,
                    "Requesting logs for blocks [{}, {}], {}", start, end, filter
                );
                let res = eth
                    .logs_with_sigs(
                        logger.cheap_clone(),
                        subgraph_metrics.cheap_clone(),
                        start,
                        end,
                        filter.cheap_clone(),
                        TOO_MANY_LOGS_FINGERPRINTS,
                    )
                    .await;

                match res {
                    Err(e) => {
                        let string_err = e.to_string();

                        // If the step is already 0, the request is too heavy even for a single
                        // block. We hope this never happens, but if it does, make sure to error.
                        if TOO_MANY_LOGS_FINGERPRINTS
                            .iter()
                            .any(|f| string_err.contains(f))
                            && step > 0
                        {
                            // The range size for a request is `step + 1`. So it's ok if the step
                            // goes down to 0, in that case we'll request one block at a time.
                            let new_step = step / 10;
                            debug!(logger, "Reducing block range size to scan for events";
                                               "new_size" => new_step + 1);
                            Ok(Some((vec![], (start, new_step))))
                        } else {
                            warn!(logger, "Unexpected RPC error"; "error" => &string_err);
                            Err(anyhow!("{}", string_err))
                        }
                    }
                    Ok(logs) => Ok(Some((logs, (end + 1, step)))),
                }
            }
        })
        .try_concat()
        .boxed()
    }

    fn call(
        &self,
        logger: Logger,
        contract_address: Address,
        call_data: Bytes,
        block_ptr: BlockPtr,
        gas: Option<u32>,
    ) -> impl Future<Item = Bytes, Error = EthereumContractCallError> + Send {
        let web3 = self.web3.clone();
        let logger = Logger::new(&logger, o!("provider" => self.provider.clone()));

        // Ganache does not support calls by block hash.
        // See https://github.com/trufflesuite/ganache-cli/issues/973
        let block_id = if !self.supports_eip_1898 {
            BlockId::Number(block_ptr.number.into())
        } else {
            BlockId::Hash(block_ptr.hash_as_h256())
        };
        let retry_log_message = format!("eth_call RPC call for block {}", block_ptr);
        retry(retry_log_message, &logger)
            .when(|result| match result {
                Ok(_) | Err(EthereumContractCallError::Revert(_)) => false,
                Err(_) => true,
            })
            .limit(ENV_VARS.request_retries)
            .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
            .run(move || {
                let call_data = call_data.clone();
                let web3 = web3.cheap_clone();
                async move {
                    let req = CallRequest {
                        to: Some(contract_address),
                        gas: gas.map(|val| web3::types::U256::from(val)),
                        data: Some(call_data.clone()),
                        from: None,
                        gas_price: None,
                        value: None,
                        access_list: None,
                        max_fee_per_gas: None,
                        max_priority_fee_per_gas: None,
                        transaction_type: None,
                    };
                    let result = web3.eth().call(req, Some(block_id)).boxed().await;

                    // Try to check if the call was reverted. The JSON-RPC response for reverts is
                    // not standardized, so we have ad-hoc checks for each Ethereum client.

                    // 0xfe is the "designated bad instruction" of the EVM, and Solidity uses it for
                    // asserts.
                    const PARITY_BAD_INSTRUCTION_FE: &str = "Bad instruction fe";

                    // 0xfd is REVERT, but on some contracts, and only on older blocks,
                    // this happens. Makes sense to consider it a revert as well.
                    const PARITY_BAD_INSTRUCTION_FD: &str = "Bad instruction fd";

                    const PARITY_BAD_JUMP_PREFIX: &str = "Bad jump";
                    const PARITY_STACK_LIMIT_PREFIX: &str = "Out of stack";

                    // See f0af4ab0-6b7c-4b68-9141-5b79346a5f61.
                    const PARITY_OUT_OF_GAS: &str = "Out of gas";

                    const PARITY_VM_EXECUTION_ERROR: i64 = -32015;
                    const PARITY_REVERT_PREFIX: &str = "Reverted 0x";
                    const XDAI_REVERT: &str = "revert";

                    // Deterministic Geth execution errors. We might need to expand this as
                    // subgraphs come across other errors. See
                    // https://github.com/ethereum/go-ethereum/blob/cd57d5cd38ef692de8fbedaa56598b4e9fbfbabc/core/vm/errors.go
                    const GETH_EXECUTION_ERRORS: &[&str] = &[
                        // The "revert" substring covers a few known error messages, including:
                        // Hardhat: "error: transaction reverted",
                        // Ganache and Moonbeam: "vm exception while processing transaction: revert",
                        // Geth: "execution reverted"
                        // And others.
                        "revert",
                        "invalid jump destination",
                        "invalid opcode",
                        // Ethereum says 1024 is the stack sizes limit, so this is deterministic.
                        "stack limit reached 1024",
                        // See f0af4ab0-6b7c-4b68-9141-5b79346a5f61 for why the gas limit is considered deterministic.
                        "out of gas",
                        "stack underflow",
                    ];

                    let env_geth_call_errors = ENV_VARS.geth_eth_call_errors.iter();
                    let mut geth_execution_errors = GETH_EXECUTION_ERRORS
                        .iter()
                        .copied()
                        .chain(env_geth_call_errors.map(|s| s.as_str()));

                    let as_solidity_revert_with_reason = |bytes: &[u8]| {
                        let solidity_revert_function_selector =
                            &tiny_keccak::keccak256(b"Error(string)")[..4];

                        match bytes.len() >= 4 && &bytes[..4] == solidity_revert_function_selector {
                            false => None,
                            true => ethabi::decode(&[ParamType::String], &bytes[4..])
                                .ok()
                                .and_then(|tokens| tokens[0].clone().into_string()),
                        }
                    };

                    match result {
                        // A successful response.
                        Ok(bytes) => Ok(bytes),

                        // Check for Geth revert.
                        Err(web3::Error::Rpc(rpc_error))
                            if geth_execution_errors
                                .any(|e| rpc_error.message.to_lowercase().contains(e)) =>
                        {
                            Err(EthereumContractCallError::Revert(rpc_error.message))
                        }

                        // Check for Parity revert.
                        Err(web3::Error::Rpc(ref rpc_error))
                            if rpc_error.code.code() == PARITY_VM_EXECUTION_ERROR =>
                        {
                            match rpc_error.data.as_ref().and_then(|d| d.as_str()) {
                                Some(data)
                                    if data.starts_with(PARITY_REVERT_PREFIX)
                                        || data.starts_with(PARITY_BAD_JUMP_PREFIX)
                                        || data.starts_with(PARITY_STACK_LIMIT_PREFIX)
                                        || data == PARITY_BAD_INSTRUCTION_FE
                                        || data == PARITY_BAD_INSTRUCTION_FD
                                        || data == PARITY_OUT_OF_GAS
                                        || data == XDAI_REVERT =>
                                {
                                    let reason = if data == PARITY_BAD_INSTRUCTION_FE {
                                        PARITY_BAD_INSTRUCTION_FE.to_owned()
                                    } else {
                                        let payload = data.trim_start_matches(PARITY_REVERT_PREFIX);
                                        hex::decode(payload)
                                            .ok()
                                            .and_then(|payload| {
                                                as_solidity_revert_with_reason(&payload)
                                            })
                                            .unwrap_or("no reason".to_owned())
                                    };
                                    Err(EthereumContractCallError::Revert(reason))
                                }

                                // The VM execution error was not identified as a revert.
                                _ => Err(EthereumContractCallError::Web3Error(web3::Error::Rpc(
                                    rpc_error.clone(),
                                ))),
                            }
                        }

                        // The error was not identified as a revert.
                        Err(err) => Err(EthereumContractCallError::Web3Error(err)),
                    }
                }
            })
            .map_err(|e| e.into_inner().unwrap_or(EthereumContractCallError::Timeout))
            .boxed()
            .compat()
    }

    /// Request blocks by hash through JSON-RPC.
    fn load_blocks_rpc(
        &self,
        logger: Logger,
        ids: Vec<H256>,
    ) -> impl Stream<Item = Arc<LightEthereumBlock>, Error = Error> + Send {
        let web3 = self.web3.clone();

        stream::iter_ok::<_, Error>(ids.into_iter().map(move |hash| {
            let web3 = web3.clone();
            retry(format!("load block {}", hash), &logger)
                .limit(ENV_VARS.request_retries)
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    Box::pin(web3.eth().block_with_txs(BlockId::Hash(hash)))
                        .compat()
                        .from_err::<Error>()
                        .and_then(move |block| {
                            block.map(Arc::new).ok_or_else(|| {
                                anyhow::anyhow!("Ethereum node did not find block {:?}", hash)
                            })
                        })
                        .compat()
                })
                .boxed()
                .compat()
                .from_err()
        }))
        .buffered(ENV_VARS.block_batch_size)
    }

    /// Request blocks ptrs for numbers through JSON-RPC.
    ///
    /// Reorg safety: If ids are numbers, they must be a final blocks.
    fn load_block_ptrs_rpc(
        &self,
        logger: Logger,
        block_nums: Vec<BlockNumber>,
    ) -> impl Stream<Item = BlockPtr, Error = Error> + Send {
        let web3 = self.web3.clone();

        stream::iter_ok::<_, Error>(block_nums.into_iter().map(move |block_num| {
            let web3 = web3.clone();
            retry(format!("load block ptr {}", block_num), &logger)
                .no_limit()
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    let web3 = web3.clone();
                    async move {
                        let block = web3
                            .eth()
                            .block(BlockId::Number(Web3BlockNumber::Number(block_num.into())))
                            .boxed()
                            .await?;

                        block.ok_or_else(|| {
                            anyhow!("Ethereum node did not find block {:?}", block_num)
                        })
                    }
                })
                .boxed()
                .compat()
                .from_err()
        }))
        .buffered(ENV_VARS.block_batch_size)
        .map(|b| b.into())
    }

    /// Check if `block_ptr` refers to a block that is on the main chain, according to the Ethereum
    /// node.
    ///
    /// Careful: don't use this function without considering race conditions.
    /// Chain reorgs could happen at any time, and could affect the answer received.
    /// Generally, it is only safe to use this function with blocks that have received enough
    /// confirmations to guarantee no further reorgs, **and** where the Ethereum node is aware of
    /// those confirmations.
    /// If the Ethereum node is far behind in processing blocks, even old blocks can be subject to
    /// reorgs.
    pub(crate) async fn is_on_main_chain(
        &self,
        logger: &Logger,
        block_ptr: BlockPtr,
    ) -> Result<bool, Error> {
        let block_hash = self
            .block_hash_by_block_number(logger, block_ptr.number)
            .compat()
            .await?;
        block_hash
            .ok_or_else(|| anyhow!("Ethereum node is missing block #{}", block_ptr.number))
            .map(|block_hash| block_hash == block_ptr.hash_as_h256())
    }

    pub(crate) fn logs_in_block_range(
        &self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: BlockNumber,
        to: BlockNumber,
        log_filter: EthereumLogFilter,
    ) -> DynTryFuture<'static, Vec<Log>, Error> {
        let eth: Self = self.cheap_clone();
        let logger = logger.clone();

        futures03::stream::iter(log_filter.eth_get_logs_filters().map(move |filter| {
            eth.cheap_clone().log_stream(
                logger.cheap_clone(),
                subgraph_metrics.cheap_clone(),
                from,
                to,
                filter,
            )
        }))
        // Real limits on the number of parallel requests are imposed within the adapter.
        .buffered(ENV_VARS.block_ingestor_max_concurrent_json_rpc_calls)
        .try_concat()
        .boxed()
    }

    pub(crate) fn calls_in_block_range<'a>(
        &self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: BlockNumber,
        to: BlockNumber,
        call_filter: &'a EthereumCallFilter,
    ) -> Box<dyn Stream<Item = EthereumCall, Error = Error> + Send + 'a> {
        let eth = self.clone();

        let EthereumCallFilter {
            contract_addresses_function_signatures,
            wildcard_signatures,
        } = call_filter;

        let mut addresses: Vec<H160> = contract_addresses_function_signatures
            .iter()
            .filter(|(_addr, (start_block, _fsigs))| start_block <= &to)
            .map(|(addr, (_start_block, _fsigs))| *addr)
            .collect::<HashSet<H160>>()
            .into_iter()
            .collect::<Vec<H160>>();

        if addresses.is_empty() && wildcard_signatures.is_empty() {
            // The filter has no started data sources in the requested range, nothing to do.
            // This prevents an expensive call to `trace_filter` with empty `addresses`.
            return Box::new(stream::empty());
        }

        // if wildcard_signatures is on, we can't filter by topic so we need to get all the traces.
        if addresses.len() > 100 || !wildcard_signatures.is_empty() {
            // If the address list is large, request all traces, this avoids generating huge
            // requests and potentially getting 413 errors.
            addresses = vec![];
        }

        Box::new(
            eth.trace_stream(logger, subgraph_metrics, from, to, addresses)
                .filter_map(|trace| EthereumCall::try_from_trace(&trace))
                .filter(move |call| {
                    // `trace_filter` can only filter by calls `to` an address and
                    // a block range. Since subgraphs are subscribing to calls
                    // for a specific contract function an additional filter needs
                    // to be applied
                    call_filter.matches(call)
                }),
        )
    }

    // Used to get the block triggers with a `polling` or `once` filter
    /// `polling_filter_type` is used to differentiate between `polling` and `once` filters
    /// A `polling_filter_type` value of  `BlockPollingFilterType::Once` is the case for
    /// intialization triggers
    /// A `polling_filter_type` value of  `BlockPollingFilterType::Polling` is the case for
    /// polling triggers
    pub(crate) fn blocks_matching_polling_intervals(
        &self,
        logger: Logger,
        from: i32,
        to: i32,
        filter: &EthereumBlockFilter,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<EthereumTrigger>, anyhow::Error>>
                + std::marker::Send,
        >,
    > {
        // Create a HashMap of block numbers to Vec<EthereumBlockTriggerType>
        let matching_blocks = (from..=to)
            .filter_map(|block_number| {
                filter
                    .polling_intervals
                    .iter()
                    .find_map(|(start_block, interval)| {
                        let has_once_trigger = (*interval == 0) && (block_number == *start_block);
                        let has_polling_trigger = block_number >= *start_block
                            && *interval > 0
                            && ((block_number - start_block) % *interval) == 0;

                        if has_once_trigger || has_polling_trigger {
                            let mut triggers = Vec::new();
                            if has_once_trigger {
                                triggers.push(EthereumBlockTriggerType::Start);
                            }
                            if has_polling_trigger {
                                triggers.push(EthereumBlockTriggerType::End);
                            }
                            Some((block_number, triggers))
                        } else {
                            None
                        }
                    })
            })
            .collect::<HashMap<_, _>>();

        let blocks_matching_polling_filter = self.load_ptrs_for_blocks(
            logger.clone(),
            matching_blocks.iter().map(|(k, _)| *k).collect_vec(),
        );

        let block_futures = blocks_matching_polling_filter.map(move |ptrs| {
            ptrs.into_iter()
                .flat_map(|ptr| {
                    let triggers = matching_blocks
                        .get(&ptr.number)
                        // Safe to unwrap since we are iterating over ptrs which was created from
                        // the keys of matching_blocks
                        .unwrap()
                        .iter()
                        .map(move |trigger| EthereumTrigger::Block(ptr.clone(), trigger.clone()));

                    triggers
                })
                .collect::<Vec<_>>()
        });

        block_futures.compat().boxed()
    }

    pub(crate) async fn calls_in_block(
        &self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        block_number: BlockNumber,
        block_hash: H256,
    ) -> Result<Vec<EthereumCall>, Error> {
        let eth = self.clone();
        let addresses = Vec::new();
        let traces = eth
            .trace_stream(
                logger,
                subgraph_metrics.clone(),
                block_number,
                block_number,
                addresses,
            )
            .collect()
            .compat()
            .await?;

        // `trace_stream` returns all of the traces for the block, and this
        // includes a trace for the block reward which every block should have.
        // If there are no traces something has gone wrong.
        if traces.is_empty() {
            return Err(anyhow!(
                "Trace stream returned no traces for block: number = `{}`, hash = `{}`",
                block_number,
                block_hash,
            ));
        }

        // Since we can only pull traces by block number and we have
        // all the traces for the block, we need to ensure that the
        // block hash for the traces is equal to the desired block hash.
        // Assume all traces are for the same block.
        if traces.iter().nth(0).unwrap().block_hash != block_hash {
            return Err(anyhow!(
                "Trace stream returned traces for an unexpected block: \
                         number = `{}`, hash = `{}`",
                block_number,
                block_hash,
            ));
        }

        Ok(traces
            .iter()
            .filter_map(EthereumCall::try_from_trace)
            .collect())
    }

    /// Reorg safety: `to` must be a final block.
    pub(crate) fn block_range_to_ptrs(
        &self,
        logger: Logger,
        from: BlockNumber,
        to: BlockNumber,
    ) -> Box<dyn Future<Item = Vec<BlockPtr>, Error = Error> + Send> {
        // Currently we can't go to the DB for this because there might be duplicate entries for
        // the same block number.
        debug!(&logger, "Requesting hashes for blocks [{}, {}]", from, to);
        Box::new(
            self.load_block_ptrs_rpc(logger, (from..=to).collect())
                .collect(),
        )
    }

    pub(crate) fn load_ptrs_for_blocks(
        &self,
        logger: Logger,
        blocks: Vec<BlockNumber>,
    ) -> Box<dyn Future<Item = Vec<BlockPtr>, Error = Error> + Send> {
        // Currently we can't go to the DB for this because there might be duplicate entries for
        // the same block number.
        debug!(&logger, "Requesting hashes for blocks {:?}", blocks);
        Box::new(self.load_block_ptrs_rpc(logger, blocks).collect())
    }

    pub async fn chain_id(&self) -> Result<u64, Error> {
        let logger = self.logger.clone();
        let web3 = self.web3.clone();
        u64::try_from(
            retry("chain_id RPC call", &logger)
                .no_limit()
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    let web3 = web3.cheap_clone();
                    async move { web3.eth().chain_id().await }
                })
                .await?,
        )
        .map_err(Error::msg)
    }
}

#[async_trait]
impl EthereumAdapterTrait for EthereumAdapter {
    fn provider(&self) -> &str {
        &self.provider
    }

    async fn net_identifiers(&self) -> Result<ChainIdentifier, Error> {
        let logger = self.logger.clone();

        let web3 = self.web3.clone();
        let metrics = self.metrics.clone();
        let provider = self.provider().to_string();
        let net_version_future = retry("net_version RPC call", &logger)
            .no_limit()
            .timeout_secs(20)
            .run(move || {
                let web3 = web3.cheap_clone();
                let metrics = metrics.cheap_clone();
                let provider = provider.clone();
                async move {
                    web3.net().version().await.map_err(|e| {
                        metrics.set_status(ProviderStatus::VersionFail, &provider);
                        e.into()
                    })
                }
            })
            .map_err(|e| {
                self.metrics
                    .set_status(ProviderStatus::VersionTimeout, self.provider());
                e
            })
            .boxed();

        let web3 = self.web3.clone();
        let metrics = self.metrics.clone();
        let provider = self.provider().to_string();
        let retry_log_message = format!(
            "eth_getBlockByNumber({}, false) RPC call",
            ENV_VARS.genesis_block_number
        );
        let gen_block_hash_future = retry(retry_log_message, &logger)
            .no_limit()
            .timeout_secs(30)
            .run(move || {
                let web3 = web3.cheap_clone();
                let metrics = metrics.cheap_clone();
                let provider = provider.clone();
                async move {
                    web3.eth()
                        .block(BlockId::Number(Web3BlockNumber::Number(
                            ENV_VARS.genesis_block_number.into(),
                        )))
                        .await
                        .map_err(|e| {
                            metrics.set_status(ProviderStatus::GenesisFail, &provider);
                            e
                        })?
                        .and_then(|gen_block| gen_block.hash.map(BlockHash::from))
                        .ok_or_else(|| anyhow!("Ethereum node could not find genesis block"))
                }
            })
            .map_err(|e| {
                self.metrics
                    .set_status(ProviderStatus::GenesisTimeout, self.provider());
                e
            });

        let (net_version, genesis_block_hash) =
            try_join!(net_version_future, gen_block_hash_future).map_err(|e| {
                anyhow!(
                    "Ethereum node took too long to read network identifiers: {}",
                    e
                )
            })?;

        let ident = ChainIdentifier {
            net_version,
            genesis_block_hash,
        };

        self.metrics
            .set_status(ProviderStatus::Working, self.provider());
        Ok(ident)
    }

    fn latest_block_header(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = web3::types::Block<H256>, Error = IngestorError> + Send> {
        let web3 = self.web3.clone();
        Box::new(
            retry("eth_getBlockByNumber(latest) no txs RPC call", logger)
                .no_limit()
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    let web3 = web3.cheap_clone();
                    async move {
                        let block_opt = web3
                            .eth()
                            .block(Web3BlockNumber::Latest.into())
                            .await
                            .map_err(|e| {
                                anyhow!("could not get latest block from Ethereum: {}", e)
                            })?;

                        block_opt
                            .ok_or_else(|| anyhow!("no latest block returned from Ethereum").into())
                    }
                })
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!("Ethereum node took too long to return latest block").into()
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn latest_block(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = IngestorError> + Send + Unpin> {
        let web3 = self.web3.clone();
        Box::new(
            retry("eth_getBlockByNumber(latest) with txs RPC call", logger)
                .no_limit()
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    let web3 = web3.cheap_clone();
                    async move {
                        let block_opt = web3
                            .eth()
                            .block_with_txs(Web3BlockNumber::Latest.into())
                            .await
                            .map_err(|e| {
                                anyhow!("could not get latest block from Ethereum: {}", e)
                            })?;
                        block_opt
                            .ok_or_else(|| anyhow!("no latest block returned from Ethereum").into())
                    }
                })
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!("Ethereum node took too long to return latest block").into()
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn load_block(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = Error> + Send> {
        Box::new(
            self.block_by_hash(logger, block_hash)
                .and_then(move |block_opt| {
                    block_opt.ok_or_else(move || {
                        anyhow!(
                            "Ethereum node could not find block with hash {}",
                            block_hash
                        )
                    })
                }),
        )
    }

    fn block_by_hash(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send> {
        let web3 = self.web3.clone();
        let logger = logger.clone();
        let retry_log_message = format!(
            "eth_getBlockByHash RPC call for block hash {:?}",
            block_hash
        );
        Box::new(
            retry(retry_log_message, &logger)
                .limit(ENV_VARS.request_retries)
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    Box::pin(web3.eth().block_with_txs(BlockId::Hash(block_hash)))
                        .compat()
                        .from_err()
                        .compat()
                })
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!("Ethereum node took too long to return block {}", block_hash)
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn block_by_number(
        &self,
        logger: &Logger,
        block_number: BlockNumber,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send> {
        let web3 = self.web3.clone();
        let logger = logger.clone();
        let retry_log_message = format!(
            "eth_getBlockByNumber RPC call for block number {}",
            block_number
        );
        Box::new(
            retry(retry_log_message, &logger)
                .no_limit()
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    let web3 = web3.cheap_clone();
                    async move {
                        web3.eth()
                            .block_with_txs(BlockId::Number(block_number.into()))
                            .await
                            .map_err(Error::from)
                    }
                })
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!(
                            "Ethereum node took too long to return block {}",
                            block_number
                        )
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn load_full_block(
        &self,
        logger: &Logger,
        block: LightEthereumBlock,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<EthereumBlock, IngestorError>> + Send>>
    {
        let web3 = Arc::clone(&self.web3);
        let logger = logger.clone();
        let block_hash = block.hash.expect("block is missing block hash");

        // The early return is necessary for correctness, otherwise we'll
        // request an empty batch which is not valid in JSON-RPC.
        if block.transactions.is_empty() {
            trace!(logger, "Block {} contains no transactions", block_hash);
            return Box::pin(std::future::ready(Ok(EthereumBlock {
                block: Arc::new(block),
                transaction_receipts: Vec::new(),
            })));
        }
        let hashes: Vec<_> = block.transactions.iter().map(|txn| txn.hash).collect();
        let receipts_future = if ENV_VARS.fetch_receipts_in_batches {
            // Deprecated batching retrieval of transaction receipts.
            fetch_transaction_receipts_in_batch_with_retry(web3, hashes, block_hash, logger).boxed()
        } else {
            let hash_stream = graph::tokio_stream::iter(hashes);
            let receipt_stream = graph::tokio_stream::StreamExt::map(hash_stream, move |tx_hash| {
                fetch_transaction_receipt_with_retry(
                    web3.cheap_clone(),
                    tx_hash,
                    block_hash,
                    logger.cheap_clone(),
                )
            })
            .buffered(ENV_VARS.block_ingestor_max_concurrent_json_rpc_calls);
            graph::tokio_stream::StreamExt::collect::<
                Result<Vec<Arc<TransactionReceipt>>, IngestorError>,
            >(receipt_stream)
            .boxed()
        };

        let block_future =
            futures03::TryFutureExt::map_ok(receipts_future, move |transaction_receipts| {
                EthereumBlock {
                    block: Arc::new(block),
                    transaction_receipts,
                }
            });

        Box::pin(block_future)
    }

    fn block_pointer_from_number(
        &self,
        logger: &Logger,
        block_number: BlockNumber,
    ) -> Box<dyn Future<Item = BlockPtr, Error = IngestorError> + Send> {
        Box::new(
            self.block_hash_by_block_number(logger, block_number)
                .and_then(move |block_hash_opt| {
                    block_hash_opt.ok_or_else(|| {
                        anyhow!(
                            "Ethereum node could not find start block hash by block number {}",
                            &block_number
                        )
                    })
                })
                .from_err()
                .map(move |block_hash| BlockPtr::from((block_hash, block_number))),
        )
    }

    fn block_hash_by_block_number(
        &self,
        logger: &Logger,
        block_number: BlockNumber,
    ) -> Box<dyn Future<Item = Option<H256>, Error = Error> + Send> {
        let web3 = self.web3.clone();
        let retry_log_message = format!(
            "eth_getBlockByNumber RPC call for block number {}",
            block_number
        );
        Box::new(
            retry(retry_log_message, logger)
                .no_limit()
                .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
                .run(move || {
                    let web3 = web3.cheap_clone();
                    async move {
                        web3.eth()
                            .block(BlockId::Number(block_number.into()))
                            .await
                            .map(|block_opt| block_opt.and_then(|block| block.hash))
                            .map_err(Error::from)
                    }
                })
                .boxed()
                .compat()
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!(
                            "Ethereum node took too long to return data for block #{}",
                            block_number
                        )
                    })
                }),
        )
    }

    fn contract_call(
        &self,
        logger: &Logger,
        call: EthereumContractCall,
        cache: Arc<dyn EthereumCallCache>,
    ) -> Box<dyn Future<Item = Vec<Token>, Error = EthereumContractCallError> + Send> {
        // Emit custom error for type mismatches.
        for (token, kind) in call
            .args
            .iter()
            .zip(call.function.inputs.iter().map(|p| &p.kind))
        {
            if !token.type_check(kind) {
                return Box::new(future::err(EthereumContractCallError::TypeError(
                    token.clone(),
                    kind.clone(),
                )));
            }
        }

        // Encode the call parameters according to the ABI
        let call_data = match call.function.encode_input(&call.args) {
            Ok(data) => data,
            Err(e) => return Box::new(future::err(EthereumContractCallError::EncodingError(e))),
        };

        debug!(logger, "eth_call";
            "address" => hex::encode(call.address),
            "data" => hex::encode(&call_data),
            "block_hash" => call.block_ptr.hash_hex(),
            "block_number" => call.block_ptr.block_number()
        );

        // Check if we have it cached, if not do the call and cache.
        Box::new(
            match cache
                .get_call(call.address, &call_data, call.block_ptr.clone())
                .map_err(|e| error!(logger, "call cache get error"; "error" => e.to_string()))
                .ok()
                .flatten()
            {
                Some(result) => {
                    Box::new(future::ok(result)) as Box<dyn Future<Item = _, Error = _> + Send>
                }
                None => {
                    let cache = cache.clone();
                    let call = call.clone();
                    let logger = logger.clone();
                    Box::new(
                        self.call(
                            logger.clone(),
                            call.address,
                            Bytes(call_data.clone()),
                            call.block_ptr.clone(),
                            call.gas,
                        )
                        .map(move |result| {
                            // Don't block handler execution on writing to the cache.
                            let for_cache = result.0.clone();
                            let _ = graph::spawn_blocking_allow_panic(move || {
                                cache
                                    .set_call(call.address, &call_data, call.block_ptr, &for_cache)
                                    .map_err(|e| {
                                        error!(logger, "call cache set error";
                                                   "error" => e.to_string())
                                    })
                            });
                            result.0
                        }),
                    )
                }
            }
            // Decode the return values according to the ABI
            .and_then(move |output| {
                if output.is_empty() {
                    // We got a `0x` response. For old Geth, this can mean a revert. It can also be
                    // that the contract actually returned an empty response. A view call is meant
                    // to return something, so we treat empty responses the same as reverts.
                    Err(EthereumContractCallError::Revert("empty response".into()))
                } else {
                    // Decode failures are reverts. The reasoning is that if Solidity fails to
                    // decode an argument, that's a revert, so the same goes for the output.
                    call.function.decode_output(&output).map_err(|e| {
                        EthereumContractCallError::Revert(format!("failed to decode output: {}", e))
                    })
                }
            }),
        )
    }

    /// Load Ethereum blocks in bulk, returning results as they come back as a Stream.
    fn load_blocks(
        &self,
        logger: Logger,
        chain_store: Arc<dyn ChainStore>,
        block_hashes: HashSet<H256>,
    ) -> Box<dyn Stream<Item = Arc<LightEthereumBlock>, Error = Error> + Send> {
        let block_hashes: Vec<_> = block_hashes.iter().cloned().collect();
        // Search for the block in the store first then use json-rpc as a backup.
        let mut blocks: Vec<Arc<LightEthereumBlock>> = chain_store
            .blocks(&block_hashes.iter().map(|&b| b.into()).collect::<Vec<_>>())
            .map_err(|e| error!(&logger, "Error accessing block cache {}", e))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| json::from_value(value).ok())
            .map(Arc::new)
            .collect();

        let missing_blocks = Vec::from_iter(
            block_hashes
                .into_iter()
                .filter(|hash| !blocks.iter().any(|b| b.hash == Some(*hash))),
        );

        // Return a stream that lazily loads batches of blocks.
        debug!(logger, "Requesting {} block(s)", missing_blocks.len());
        Box::new(
            self.load_blocks_rpc(logger.clone(), missing_blocks)
                .collect()
                .map(move |new_blocks| {
                    let upsert_blocks: Vec<_> = new_blocks
                        .iter()
                        .map(|block| BlockFinality::Final(block.clone()))
                        .collect();
                    let block_refs: Vec<_> = upsert_blocks
                        .iter()
                        .map(|block| block as &dyn graph::blockchain::Block)
                        .collect();
                    if let Err(e) = chain_store.upsert_light_blocks(block_refs.as_slice()) {
                        error!(logger, "Error writing to block cache {}", e);
                    }
                    blocks.extend(new_blocks);
                    blocks.sort_by_key(|block| block.number);
                    stream::iter_ok(blocks)
                })
                .flatten_stream(),
        )
    }
}

/// Returns blocks with triggers, corresponding to the specified range and filters.
/// If a block contains no triggers, there may be no corresponding item in the stream.
/// However the `to` block will always be present, even if triggers are empty.
///
/// Careful: don't use this function without considering race conditions.
/// Chain reorgs could happen at any time, and could affect the answer received.
/// Generally, it is only safe to use this function with blocks that have received enough
/// confirmations to guarantee no further reorgs, **and** where the Ethereum node is aware of
/// those confirmations.
/// If the Ethereum node is far behind in processing blocks, even old blocks can be subject to
/// reorgs.
/// It is recommended that `to` be far behind the block number of latest block the Ethereum
/// node is aware of.
pub(crate) async fn blocks_with_triggers(
    adapter: Arc<EthereumAdapter>,
    logger: Logger,
    chain_store: Arc<dyn ChainStore>,
    subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
    from: BlockNumber,
    to: BlockNumber,
    filter: &TriggerFilter,
    unified_api_version: UnifiedMappingApiVersion,
) -> Result<Vec<BlockWithTriggers<crate::Chain>>, Error> {
    // Each trigger filter needs to be queried for the same block range
    // and the blocks yielded need to be deduped. If any error occurs
    // while searching for a trigger type, the entire operation fails.
    let eth = adapter.clone();
    let call_filter = EthereumCallFilter::from(&filter.block);

    // Scan the block range to find relevant triggers
    let trigger_futs: FuturesUnordered<BoxFuture<Result<Vec<EthereumTrigger>, anyhow::Error>>> =
        FuturesUnordered::new();

    // This is for `start` triggers which can be initialization handlers which needs to be run
    // before all other triggers
    if filter.block.trigger_every_block {
        let block_future = eth
            .block_range_to_ptrs(logger.clone(), from, to)
            .map(move |ptrs| {
                ptrs.into_iter()
                    .flat_map(|ptr| {
                        vec![
                            EthereumTrigger::Block(ptr.clone(), EthereumBlockTriggerType::Start),
                            EthereumTrigger::Block(ptr, EthereumBlockTriggerType::End),
                        ]
                    })
                    .collect()
            })
            .compat()
            .boxed();
        trigger_futs.push(block_future)
    } else if !filter.block.polling_intervals.is_empty() {
        let block_futures_matching_once_filter =
            eth.blocks_matching_polling_intervals(logger.clone(), from, to, &filter.block);
        trigger_futs.push(block_futures_matching_once_filter);
    }

    // Scan for Logs
    if !filter.log.is_empty() {
        let logs_future = get_logs_and_transactions(
            &eth,
            &logger,
            subgraph_metrics.clone(),
            from,
            to,
            filter.log.clone(),
            &unified_api_version,
        )
        .boxed();
        trigger_futs.push(logs_future)
    }
    // Scan for Calls
    if !filter.call.is_empty() {
        let calls_future = eth
            .calls_in_block_range(&logger, subgraph_metrics.clone(), from, to, &filter.call)
            .map(Arc::new)
            .map(EthereumTrigger::Call)
            .collect()
            .compat()
            .boxed();
        trigger_futs.push(calls_future)
    }

    if !filter.block.contract_addresses.is_empty() {
        // To determine which blocks include a call to addresses
        // in the block filter, transform the `block_filter` into
        // a `call_filter` and run `blocks_with_calls`
        let block_future = eth
            .calls_in_block_range(&logger, subgraph_metrics.clone(), from, to, &call_filter)
            .map(|call| {
                EthereumTrigger::Block(
                    BlockPtr::from(&call),
                    EthereumBlockTriggerType::WithCallTo(call.to),
                )
            })
            .collect()
            .compat()
            .boxed();
        trigger_futs.push(block_future)
    }

    // Get hash for "to" block
    let to_hash_fut = eth
        .block_hash_by_block_number(&logger, to)
        .and_then(|hash| match hash {
            Some(hash) => Ok(hash),
            None => {
                warn!(logger,
                      "Ethereum endpoint is behind";
                      "url" => eth.provider()
                );
                bail!("Block {} not found in the chain", to)
            }
        })
        .compat();

    // Join on triggers and block hash resolution
    let (triggers, to_hash) = futures03::join!(trigger_futs.try_concat(), to_hash_fut);

    // Unpack and handle possible errors in the previously joined futures
    let triggers =
        triggers.with_context(|| format!("Failed to obtain triggers for block {}", to))?;
    let to_hash = to_hash.with_context(|| format!("Failed to infer hash for block {}", to))?;

    let mut block_hashes: HashSet<H256> =
        triggers.iter().map(EthereumTrigger::block_hash).collect();
    let mut triggers_by_block: HashMap<BlockNumber, Vec<EthereumTrigger>> =
        triggers.into_iter().fold(HashMap::new(), |mut map, t| {
            map.entry(t.block_number()).or_default().push(t);
            map
        });

    debug!(logger, "Found {} relevant block(s)", block_hashes.len());

    // Make sure `to` is included, even if empty.
    block_hashes.insert(to_hash);
    triggers_by_block.entry(to).or_default();

    let logger2 = logger.cheap_clone();

    let blocks = eth
        .load_blocks(logger.cheap_clone(), chain_store.clone(), block_hashes)
        .and_then(
            move |block| match triggers_by_block.remove(&(block.number() as BlockNumber)) {
                Some(triggers) => Ok(BlockWithTriggers::new(
                    BlockFinality::Final(block),
                    triggers,
                    &logger2,
                )),
                None => Err(anyhow!(
                    "block {} not found in `triggers_by_block`",
                    block.block_ptr()
                )),
            },
        )
        .collect()
        .compat()
        .await?;

    // Filter out call triggers that come from unsuccessful transactions
    let futures = blocks.into_iter().map(|block| {
        filter_call_triggers_from_unsuccessful_transactions(block, &eth, &chain_store, &logger)
    });
    let mut blocks = futures03::future::try_join_all(futures).await?;

    blocks.sort_by_key(|block| block.ptr().number);

    // Sanity check that the returned blocks are in the correct range.
    // Unwrap: `blocks` always includes at least `to`.
    let first = blocks.first().unwrap().ptr().number;
    let last = blocks.last().unwrap().ptr().number;
    if first < from {
        return Err(anyhow!(
            "block {} returned by the Ethereum node is before {}, the first block of the requested range",
            first,
            from,
        ));
    }
    if last > to {
        return Err(anyhow!(
            "block {} returned by the Ethereum node is after {}, the last block of the requested range",
            last,
            to,
        ));
    }

    Ok(blocks)
}

pub(crate) async fn get_calls(
    client: &Arc<ChainClient<Chain>>,
    logger: Logger,
    subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
    capabilities: &NodeCapabilities,
    requires_traces: bool,
    block: BlockFinality,
) -> Result<BlockFinality, Error> {
    // For final blocks, or nonfinal blocks where we already checked
    // (`calls.is_some()`), do nothing; if we haven't checked for calls, do
    // that now
    match block {
        BlockFinality::Final(_)
        | BlockFinality::NonFinal(EthereumBlockWithCalls {
            ethereum_block: _,
            calls: Some(_),
        }) => Ok(block),
        BlockFinality::NonFinal(EthereumBlockWithCalls {
            ethereum_block,
            calls: None,
        }) => {
            let calls = if !requires_traces || ethereum_block.transaction_receipts.is_empty() {
                vec![]
            } else {
                client
                    .rpc()?
                    .cheapest_with(capabilities)?
                    .calls_in_block(
                        &logger,
                        subgraph_metrics.clone(),
                        BlockNumber::try_from(ethereum_block.block.number.unwrap().as_u64())
                            .unwrap(),
                        ethereum_block.block.hash.unwrap(),
                    )
                    .await?
            };
            Ok(BlockFinality::NonFinal(EthereumBlockWithCalls {
                ethereum_block,
                calls: Some(calls),
            }))
        }
    }
}

pub(crate) fn parse_log_triggers(
    log_filter: &EthereumLogFilter,
    block: &EthereumBlock,
) -> Vec<EthereumTrigger> {
    if log_filter.is_empty() {
        return vec![];
    }

    block
        .transaction_receipts
        .iter()
        .flat_map(move |receipt| {
            receipt.logs.iter().enumerate().map(move |(index, _)| {
                EthereumTrigger::Log(LogRef::LogPosition(index, receipt.cheap_clone()))
            })
        })
        .collect()
}

pub(crate) fn parse_call_triggers(
    call_filter: &EthereumCallFilter,
    block: &EthereumBlockWithCalls,
) -> anyhow::Result<Vec<EthereumTrigger>> {
    if call_filter.is_empty() {
        return Ok(vec![]);
    }

    match &block.calls {
        Some(calls) => calls
            .iter()
            .filter(move |call| call_filter.matches(call))
            .map(
                move |call| match block.transaction_for_call_succeeded(call) {
                    Ok(true) => Ok(Some(EthereumTrigger::Call(Arc::new(call.clone())))),
                    Ok(false) => Ok(None),
                    Err(e) => Err(e),
                },
            )
            .filter_map_ok(|some_trigger| some_trigger)
            .collect(),
        None => Ok(vec![]),
    }
}

/// This method does not parse block triggers with `once` filters.
/// This is because it is to be run before any other triggers are run.
/// So we have `parse_initialization_triggers` for that.
pub(crate) fn parse_block_triggers(
    block_filter: &EthereumBlockFilter,
    block: &EthereumBlockWithCalls,
) -> Vec<EthereumTrigger> {
    if block_filter.is_empty() {
        return vec![];
    }

    let block_ptr = BlockPtr::from(&block.ethereum_block);
    let trigger_every_block = block_filter.trigger_every_block;
    let call_filter = EthereumCallFilter::from(block_filter);
    let block_ptr2 = block_ptr.cheap_clone();
    let block_ptr3 = block_ptr.cheap_clone();
    let block_number = block_ptr.number;

    let mut triggers = match &block.calls {
        Some(calls) => calls
            .iter()
            .filter(move |call| call_filter.matches(call))
            .map(move |call| {
                EthereumTrigger::Block(
                    block_ptr2.clone(),
                    EthereumBlockTriggerType::WithCallTo(call.to),
                )
            })
            .collect::<Vec<EthereumTrigger>>(),
        None => vec![],
    };
    if trigger_every_block {
        triggers.push(EthereumTrigger::Block(
            block_ptr.clone(),
            EthereumBlockTriggerType::Start,
        ));
        triggers.push(EthereumTrigger::Block(
            block_ptr,
            EthereumBlockTriggerType::End,
        ));
    } else if !block_filter.polling_intervals.is_empty() {
        let has_polling_trigger =
            &block_filter
                .polling_intervals
                .iter()
                .any(|(start_block, interval)| match interval {
                    0 => false,
                    _ => {
                        block_number >= *start_block
                            && (block_number - *start_block) % *interval == 0
                    }
                });

        let has_once_trigger =
            &block_filter
                .polling_intervals
                .iter()
                .any(|(start_block, interval)| match interval {
                    0 => block_number == *start_block,
                    _ => false,
                });

        if *has_once_trigger {
            triggers.push(EthereumTrigger::Block(
                block_ptr3.clone(),
                EthereumBlockTriggerType::Start,
            ));
        }

        if *has_polling_trigger {
            triggers.push(EthereumTrigger::Block(
                block_ptr3,
                EthereumBlockTriggerType::End,
            ));
        }
    }
    triggers
}

async fn fetch_receipt_from_ethereum_client(
    eth: &EthereumAdapter,
    transaction_hash: &H256,
) -> anyhow::Result<TransactionReceipt> {
    match eth.web3.eth().transaction_receipt(*transaction_hash).await {
        Ok(Some(receipt)) => Ok(receipt),
        Ok(None) => bail!("Could not find transaction receipt"),
        Err(error) => bail!("Failed to fetch transaction receipt: {}", error),
    }
}

async fn filter_call_triggers_from_unsuccessful_transactions(
    mut block: BlockWithTriggers<crate::Chain>,
    eth: &EthereumAdapter,
    chain_store: &Arc<dyn ChainStore>,
    logger: &Logger,
) -> anyhow::Result<BlockWithTriggers<crate::Chain>> {
    // Return early if there is no trigger data
    if block.trigger_data.is_empty() {
        return Ok(block);
    }

    let initial_number_of_triggers = block.trigger_data.len();

    // Get the transaction hash from each call trigger
    let transaction_hashes: BTreeSet<H256> = block
        .trigger_data
        .iter()
        .filter_map(|trigger| match trigger {
            EthereumTrigger::Call(call_trigger) => Some(call_trigger.transaction_hash),
            _ => None,
        })
        .collect::<Option<BTreeSet<H256>>>()
        .ok_or(anyhow!(
            "failed to obtain transaction hash from call triggers"
        ))?;

    // Return early if there are no transaction hashes
    if transaction_hashes.is_empty() {
        return Ok(block);
    }

    // And obtain all Transaction values for the calls in this block.
    let transactions: Vec<&Transaction> = {
        match &block.block {
            BlockFinality::Final(ref block) => block
                .transactions
                .iter()
                .filter(|transaction| transaction_hashes.contains(&transaction.hash))
                .collect(),
            BlockFinality::NonFinal(_block_with_calls) => {
                unreachable!(
                    "this function should not be called when dealing with non-final blocks"
                )
            }
        }
    };

    // Confidence check: Did we collect all transactions for the current call triggers?
    if transactions.len() != transaction_hashes.len() {
        bail!("failed to find transactions in block for the given call triggers")
    }

    // We'll also need the receipts for those transactions. In this step we collect all receipts
    // we have in store for the current block.
    let mut receipts = chain_store
        .transaction_receipts_in_block(&block.ptr().hash_as_h256())
        .await?
        .into_iter()
        .map(|receipt| (receipt.transaction_hash, receipt))
        .collect::<BTreeMap<H256, LightTransactionReceipt>>();

    // Do we have a receipt for each transaction under analysis?
    let mut receipts_and_transactions: Vec<(&Transaction, LightTransactionReceipt)> = Vec::new();
    let mut transactions_without_receipt: Vec<&Transaction> = Vec::new();
    for transaction in transactions.iter() {
        if let Some(receipt) = receipts.remove(&transaction.hash) {
            receipts_and_transactions.push((transaction, receipt));
        } else {
            transactions_without_receipt.push(transaction);
        }
    }

    // When some receipts are missing, we then try to fetch them from our client.
    let futures = transactions_without_receipt
        .iter()
        .map(|transaction| async move {
            fetch_receipt_from_ethereum_client(eth, &transaction.hash)
                .await
                .map(|receipt| (transaction, receipt))
        });
    futures03::future::try_join_all(futures)
        .await?
        .into_iter()
        .for_each(|(transaction, receipt)| {
            receipts_and_transactions.push((transaction, receipt.into()))
        });

    // TODO: We should persist those fresh transaction receipts into the store, so we don't incur
    // additional Ethereum API calls for future scans on this block.

    // With all transactions and receipts in hand, we can evaluate the success of each transaction
    let mut transaction_success: BTreeMap<&H256, bool> = BTreeMap::new();
    for (transaction, receipt) in receipts_and_transactions.into_iter() {
        transaction_success.insert(
            &transaction.hash,
            evaluate_transaction_status(receipt.status),
        );
    }

    // Confidence check: Did we inspect the status of all transactions?
    if !transaction_hashes
        .iter()
        .all(|tx| transaction_success.contains_key(tx))
    {
        bail!("Not all transactions status were inspected")
    }

    // Filter call triggers from unsuccessful transactions
    block.trigger_data.retain(|trigger| {
        if let EthereumTrigger::Call(call_trigger) = trigger {
            // Unwrap: We already checked that those values exist
            transaction_success[&call_trigger.transaction_hash.unwrap()]
        } else {
            // We are not filtering other types of triggers
            true
        }
    });

    // Log if any call trigger was filtered out
    let final_number_of_triggers = block.trigger_data.len();
    let number_of_filtered_triggers = initial_number_of_triggers - final_number_of_triggers;
    if number_of_filtered_triggers != 0 {
        let noun = {
            if number_of_filtered_triggers == 1 {
                "call trigger"
            } else {
                "call triggers"
            }
        };
        info!(&logger,
              "Filtered {} {} from failed transactions", number_of_filtered_triggers, noun ;
              "block_number" => block.ptr().block_number());
    }
    Ok(block)
}

/// Deprecated. Wraps the [`fetch_transaction_receipts_in_batch`] in a retry loop.
async fn fetch_transaction_receipts_in_batch_with_retry(
    web3: Arc<Web3<Transport>>,
    hashes: Vec<H256>,
    block_hash: H256,
    logger: Logger,
) -> Result<Vec<Arc<TransactionReceipt>>, IngestorError> {
    let retry_log_message = format!(
        "batch eth_getTransactionReceipt RPC call for block {:?}",
        block_hash
    );
    retry(retry_log_message, &logger)
        .limit(ENV_VARS.request_retries)
        .no_logging()
        .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
        .run(move || {
            let web3 = web3.cheap_clone();
            let hashes = hashes.clone();
            let logger = logger.cheap_clone();
            fetch_transaction_receipts_in_batch(web3, hashes, block_hash, logger).boxed()
        })
        .await
        .map_err(|_timeout| anyhow!(block_hash).into())
}

/// Deprecated. Attempts to fetch multiple transaction receipts in a batching contex.
async fn fetch_transaction_receipts_in_batch(
    web3: Arc<Web3<Transport>>,
    hashes: Vec<H256>,
    block_hash: H256,
    logger: Logger,
) -> Result<Vec<Arc<TransactionReceipt>>, IngestorError> {
    let batching_web3 = Web3::new(Batch::new(web3.transport().clone()));
    let eth = batching_web3.eth();
    let receipt_futures = hashes
        .into_iter()
        .map(move |hash| {
            let logger = logger.cheap_clone();
            eth.transaction_receipt(hash)
                .map_err(IngestorError::from)
                .and_then(move |some_receipt| async move {
                    resolve_transaction_receipt(some_receipt, hash, block_hash, logger)
                })
        })
        .collect::<Vec<_>>();

    batching_web3.transport().submit_batch().await?;

    let mut collected = vec![];
    for receipt in receipt_futures.into_iter() {
        collected.push(Arc::new(receipt.await?))
    }
    Ok(collected)
}

/// Retries fetching a single transaction receipt.
async fn fetch_transaction_receipt_with_retry(
    web3: Arc<Web3<Transport>>,
    transaction_hash: H256,
    block_hash: H256,
    logger: Logger,
) -> Result<Arc<TransactionReceipt>, IngestorError> {
    let logger = logger.cheap_clone();
    let retry_log_message = format!(
        "eth_getTransactionReceipt RPC call for transaction {:?}",
        transaction_hash
    );
    retry(retry_log_message, &logger)
        .limit(ENV_VARS.request_retries)
        .timeout_secs(ENV_VARS.json_rpc_timeout.as_secs())
        .run(move || web3.eth().transaction_receipt(transaction_hash).boxed())
        .await
        .map_err(|_timeout| anyhow!(block_hash).into())
        .and_then(move |some_receipt| {
            resolve_transaction_receipt(some_receipt, transaction_hash, block_hash, logger)
        })
        .map(Arc::new)
}

fn resolve_transaction_receipt(
    transaction_receipt: Option<TransactionReceipt>,
    transaction_hash: H256,
    block_hash: H256,
    logger: Logger,
) -> Result<TransactionReceipt, IngestorError> {
    match transaction_receipt {
        // A receipt might be missing because the block was uncled, and the transaction never
        // made it back into the main chain.
        Some(receipt) => {
            // Check if the receipt has a block hash and is for the right block. Parity nodes seem
            // to return receipts with no block hash when a transaction is no longer in the main
            // chain, so treat that case the same as a receipt being absent entirely.
            //
            // Also as a sanity check against provider nonsense, check that the receipt transaction
            // hash and the requested transaction hash match.
            if receipt.block_hash != Some(block_hash)
                || transaction_hash != receipt.transaction_hash
            {
                info!(
                    logger, "receipt block mismatch";
                    "receipt_block_hash" =>
                    receipt.block_hash.unwrap_or_default().to_string(),
                    "block_hash" =>
                        block_hash.to_string(),
                    "tx_hash" => transaction_hash.to_string(),
                    "receipt_tx_hash" => receipt.transaction_hash.to_string(),
                );

                // If the receipt came from a different block, then the Ethereum node no longer
                // considers this block to be in the main chain. Nothing we can do from here except
                // give up trying to ingest this block. There is no way to get the transaction
                // receipt from this block.
                Err(IngestorError::BlockUnavailable(block_hash))
            } else {
                Ok(receipt)
            }
        }
        None => {
            // No receipt was returned.
            //
            // This can be because the Ethereum node no longer considers this block to be part of
            // the main chain, and so the transaction is no longer in the main chain. Nothing we can
            // do from here except give up trying to ingest this block.
            //
            // This could also be because the receipt is simply not available yet. For that case, we
            // should retry until it becomes available.
            Err(IngestorError::ReceiptUnavailable(
                block_hash,
                transaction_hash,
            ))
        }
    }
}

/// Retrieves logs and the associated transaction receipts, if required by the [`EthereumLogFilter`].
async fn get_logs_and_transactions(
    adapter: &Arc<EthereumAdapter>,
    logger: &Logger,
    subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
    from: BlockNumber,
    to: BlockNumber,
    log_filter: EthereumLogFilter,
    unified_api_version: &UnifiedMappingApiVersion,
) -> Result<Vec<EthereumTrigger>, anyhow::Error> {
    // Obtain logs externally
    let logs = adapter
        .logs_in_block_range(
            logger,
            subgraph_metrics.cheap_clone(),
            from,
            to,
            log_filter.clone(),
        )
        .await?;

    // Not all logs have associated transaction hashes, nor do all triggers require them.
    // We also restrict receipts retrieval for some api versions.
    let transaction_hashes_by_block: HashMap<H256, HashSet<H256>> = logs
        .iter()
        .filter(|_| unified_api_version.equal_or_greater_than(&API_VERSION_0_0_7))
        .filter(|log| {
            if let Some(signature) = log.topics.first() {
                log_filter.requires_transaction_receipt(signature, Some(&log.address))
            } else {
                false
            }
        })
        .filter_map(|log| {
            if let (Some(block), Some(txn)) = (log.block_hash, log.transaction_hash) {
                Some((block, txn))
            } else {
                // Absent block and transaction data might happen for pending transactions, which we
                // don't handle.
                None
            }
        })
        .fold(
            HashMap::<H256, HashSet<H256>>::new(),
            |mut acc, (block_hash, txn_hash)| {
                acc.entry(block_hash).or_default().insert(txn_hash);
                acc
            },
        );

    // Obtain receipts externally
    let transaction_receipts_by_hash = get_transaction_receipts_for_transaction_hashes(
        adapter,
        &transaction_hashes_by_block,
        subgraph_metrics,
        logger.cheap_clone(),
    )
    .await?;

    // Associate each log with its receipt, when possible
    let mut log_triggers = Vec::new();
    for log in logs.into_iter() {
        let optional_receipt = log
            .transaction_hash
            .and_then(|txn| transaction_receipts_by_hash.get(&txn).cloned());
        let value = EthereumTrigger::Log(LogRef::FullLog(Arc::new(log), optional_receipt));
        log_triggers.push(value);
    }

    Ok(log_triggers)
}

/// Tries to retrive all transaction receipts for a set of transaction hashes.
async fn get_transaction_receipts_for_transaction_hashes(
    adapter: &EthereumAdapter,
    transaction_hashes_by_block: &HashMap<H256, HashSet<H256>>,
    subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
    logger: Logger,
) -> Result<HashMap<H256, Arc<TransactionReceipt>>, anyhow::Error> {
    use std::collections::hash_map::Entry::Vacant;

    let mut receipts_by_hash: HashMap<H256, Arc<TransactionReceipt>> = HashMap::new();

    // Return early if input set is empty
    if transaction_hashes_by_block.is_empty() {
        return Ok(receipts_by_hash);
    }

    // Keep a record of all unique transaction hashes for which we'll request receipts. We will
    // later use this to check if we have collected the receipts from all required transactions.
    let mut unique_transaction_hashes: HashSet<&H256> = HashSet::new();

    // Request transaction receipts concurrently
    let receipt_futures = FuturesUnordered::new();

    let web3 = Arc::clone(&adapter.web3);
    for (block_hash, transaction_hashes) in transaction_hashes_by_block {
        for transaction_hash in transaction_hashes {
            unique_transaction_hashes.insert(transaction_hash);
            let receipt_future = fetch_transaction_receipt_with_retry(
                web3.cheap_clone(),
                *transaction_hash,
                *block_hash,
                logger.cheap_clone(),
            );
            receipt_futures.push(receipt_future)
        }
    }

    // Execute futures while monitoring elapsed time
    let start = Instant::now();
    let receipts: Vec<_> = match receipt_futures.try_collect().await {
        Ok(receipts) => {
            let elapsed = start.elapsed().as_secs_f64();
            subgraph_metrics.observe_request(
                elapsed,
                "eth_getTransactionReceipt",
                &adapter.provider,
            );
            receipts
        }
        Err(ingestor_error) => {
            subgraph_metrics.add_error("eth_getTransactionReceipt", &adapter.provider);
            debug!(
                logger,
                "Error querying transaction receipts: {}", ingestor_error
            );
            return Err(ingestor_error.into());
        }
    };

    // Build a map between transaction hashes and their receipts
    for receipt in receipts.into_iter() {
        if !unique_transaction_hashes.remove(&receipt.transaction_hash) {
            bail!("Received a receipt for a different transaction hash")
        }
        if let Vacant(entry) = receipts_by_hash.entry(receipt.transaction_hash) {
            entry.insert(receipt);
        } else {
            bail!("Received a duplicate transaction receipt")
        }
    }

    // Confidence check: all unique hashes should have been used
    ensure!(
        unique_transaction_hashes.is_empty(),
        "Didn't receive all necessary transaction receipts"
    );

    Ok(receipts_by_hash)
}

#[cfg(test)]
mod tests {

    use crate::trigger::{EthereumBlockTriggerType, EthereumTrigger};

    use super::{parse_block_triggers, EthereumBlock, EthereumBlockFilter, EthereumBlockWithCalls};
    use graph::blockchain::BlockPtr;
    use graph::prelude::ethabi::ethereum_types::U64;
    use graph::prelude::web3::types::{Address, Block, Bytes, H256};
    use graph::prelude::EthereumCall;
    use std::collections::HashSet;
    use std::iter::FromIterator;
    use std::sync::Arc;

    #[test]
    fn parse_block_triggers_every_block() {
        let block = EthereumBlockWithCalls {
            ethereum_block: EthereumBlock {
                block: Arc::new(Block {
                    hash: Some(hash(2)),
                    number: Some(U64::from(2)),
                    ..Default::default()
                }),
                ..Default::default()
            },
            calls: Some(vec![EthereumCall {
                to: address(4),
                input: bytes(vec![1; 36]),
                ..Default::default()
            }]),
        };

        assert_eq!(
            vec![
                EthereumTrigger::Block(
                    BlockPtr::from((hash(2), 2)),
                    EthereumBlockTriggerType::Start
                ),
                EthereumTrigger::Block(BlockPtr::from((hash(2), 2)), EthereumBlockTriggerType::End)
            ],
            parse_block_triggers(
                &EthereumBlockFilter {
                    polling_intervals: HashSet::new(),
                    contract_addresses: HashSet::from_iter(vec![(10, address(1))]),
                    trigger_every_block: true,
                },
                &block
            ),
            "every block should generate a trigger even when address don't match"
        );
    }

    #[test]
    fn parse_block_triggers_specific_call_not_found() {
        let block = EthereumBlockWithCalls {
            ethereum_block: EthereumBlock {
                block: Arc::new(Block {
                    hash: Some(hash(2)),
                    number: Some(U64::from(2)),
                    ..Default::default()
                }),
                ..Default::default()
            },
            calls: Some(vec![EthereumCall {
                to: address(4),
                input: bytes(vec![1; 36]),
                ..Default::default()
            }]),
        };

        assert_eq!(
            Vec::<EthereumTrigger>::new(),
            parse_block_triggers(
                &EthereumBlockFilter {
                    polling_intervals: HashSet::new(),
                    contract_addresses: HashSet::from_iter(vec![(1, address(1))]),
                    trigger_every_block: false,
                },
                &block
            ),
            "block filter specifies address 1 but block does not contain any call to it"
        );
    }

    #[test]
    fn parse_block_triggers_specific_call_found() {
        let block = EthereumBlockWithCalls {
            ethereum_block: EthereumBlock {
                block: Arc::new(Block {
                    hash: Some(hash(2)),
                    number: Some(U64::from(2)),
                    ..Default::default()
                }),
                ..Default::default()
            },
            calls: Some(vec![EthereumCall {
                to: address(4),
                input: bytes(vec![1; 36]),
                ..Default::default()
            }]),
        };

        assert_eq!(
            vec![EthereumTrigger::Block(
                BlockPtr::from((hash(2), 2)),
                EthereumBlockTriggerType::WithCallTo(address(4))
            )],
            parse_block_triggers(
                &EthereumBlockFilter {
                    polling_intervals: HashSet::new(),
                    contract_addresses: HashSet::from_iter(vec![(1, address(4))]),
                    trigger_every_block: false,
                },
                &block
            ),
            "block filter specifies address 4 and block has call to it"
        );
    }

    fn address(id: u64) -> Address {
        Address::from_low_u64_be(id)
    }

    fn hash(id: u8) -> H256 {
        H256::from([id; 32])
    }

    fn bytes(value: Vec<u8>) -> Bytes {
        Bytes::from(value)
    }
}
