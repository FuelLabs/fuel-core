use envconfig::Envconfig;
use graph::env::EnvVarBoolean;
use graph::prelude::{envconfig, lazy_static, BlockNumber};
use std::fmt;
use std::time::Duration;

lazy_static! {
    pub static ref ENV_VARS: EnvVars = EnvVars::from_env().unwrap();
}

#[derive(Clone)]
#[non_exhaustive]
pub struct EnvVars {
    /// Additional deterministic errors that have not yet been hardcoded.
    ///
    /// Set by the environment variable `GRAPH_GETH_ETH_CALL_ERRORS`, separated
    /// by `;`.
    pub geth_eth_call_errors: Vec<String>,
    /// Set by the environment variable `GRAPH_ETH_GET_LOGS_MAX_CONTRACTS`. The
    /// default value is 2000.
    pub get_logs_max_contracts: usize,

    /// Set by the environment variable `ETHEREUM_TRACE_STREAM_STEP_SIZE`. The
    /// default value is 50 blocks.
    pub trace_stream_step_size: BlockNumber,
    /// Maximum range size for `eth.getLogs` requests that don't filter on
    /// contract address, only event signature, and are therefore expensive.
    ///
    /// Set by the environment variable `GRAPH_ETHEREUM_MAX_EVENT_ONLY_RANGE`. The
    /// default value is 500 blocks, which is reasonable according to Ethereum
    /// node operators.
    pub max_event_only_range: BlockNumber,
    /// Set by the environment variable `ETHEREUM_BLOCK_BATCH_SIZE`. The
    /// default value is 10 blocks.
    pub block_batch_size: usize,
    /// Maximum number of blocks to request in each chunk.
    ///
    /// Set by the environment variable `GRAPH_ETHEREUM_MAX_BLOCK_RANGE_SIZE`.
    /// The default value is 2000 blocks.
    pub max_block_range_size: BlockNumber,
    /// This should not be too large that it causes requests to timeout without
    /// us catching it, nor too small that it causes us to timeout requests that
    /// would've succeeded. We've seen successful `eth_getLogs` requests take
    /// over 120 seconds.
    ///
    /// Set by the environment variable `GRAPH_ETHEREUM_JSON_RPC_TIMEOUT`
    /// (expressed in seconds). The default value is 180s.
    pub json_rpc_timeout: Duration,
    /// This is used for requests that will not fail the subgraph if the limit
    /// is reached, but will simply restart the syncing step, so it can be low.
    /// This limit guards against scenarios such as requesting a block hash that
    /// has been reorged.
    ///
    /// Set by the environment variable `GRAPH_ETHEREUM_REQUEST_RETRIES`. The
    /// default value is 10.
    pub request_retries: usize,
    /// Set by the environment variable
    /// `GRAPH_ETHEREUM_BLOCK_INGESTOR_MAX_CONCURRENT_JSON_RPC_CALLS_FOR_TXN_RECEIPTS`.
    /// The default value is 1000.
    pub block_ingestor_max_concurrent_json_rpc_calls: usize,
    /// Set by the flag `GRAPH_ETHEREUM_FETCH_TXN_RECEIPTS_IN_BATCHES`. Enabled
    /// by default on macOS (to avoid DNS issues) and disabled by default on all
    /// other systems.
    pub fetch_receipts_in_batches: bool,
    /// `graph_node::config` disallows setting this in a store with multiple
    /// shards. See 8b6ad0c64e244023ac20ced7897fe666 for the reason.
    ///
    /// Set by the flag `GRAPH_ETHEREUM_CLEANUP_BLOCKS`. Off by default.
    pub cleanup_blocks: bool,
    /// Ideal number of triggers in a range. The range size will adapt to try to
    /// meet this.
    ///
    /// Set by the environment variable
    /// `GRAPH_ETHEREUM_TARGET_TRIGGERS_PER_BLOCK_RANGE`. The default value is
    /// 100.
    pub target_triggers_per_block_range: u64,
    /// These are some chains, the genesis block is start from 1 not 0. If this
    /// flag is not set, the default value will be 0.
    ///
    /// Set by the flag `GRAPH_ETHEREUM_GENESIS_BLOCK_NUMBER`. The default value
    /// is 0.
    pub genesis_block_number: u64,
    /// Set by the flag `GRAPH_ETH_CALL_NO_GAS`.
    /// This is a comma separated list of chain ids for which the gas field will not be set
    /// when calling `eth_call`.
    pub eth_call_no_gas: Vec<String>,
}

// This does not print any values avoid accidentally leaking any sensitive env vars
impl fmt::Debug for EnvVars {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "env vars")
    }
}

impl EnvVars {
    pub fn from_env() -> Result<Self, envconfig::Error> {
        Ok(Inner::init_from_env()?.into())
    }
}

impl From<Inner> for EnvVars {
    fn from(x: Inner) -> Self {
        Self {
            get_logs_max_contracts: x.get_logs_max_contracts,
            geth_eth_call_errors: x
                .geth_eth_call_errors
                .split(';')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            trace_stream_step_size: x.trace_stream_step_size,
            max_event_only_range: x.max_event_only_range,
            block_batch_size: x.block_batch_size,
            max_block_range_size: x.max_block_range_size,
            json_rpc_timeout: Duration::from_secs(x.json_rpc_timeout_in_secs),
            request_retries: x.request_retries,
            block_ingestor_max_concurrent_json_rpc_calls: x
                .block_ingestor_max_concurrent_json_rpc_calls,
            fetch_receipts_in_batches: x
                .fetch_receipts_in_batches
                .map(|b| b.0)
                .unwrap_or(cfg!(target_os = "macos")),
            cleanup_blocks: x.cleanup_blocks.0,
            target_triggers_per_block_range: x.target_triggers_per_block_range,
            genesis_block_number: x.genesis_block_number,
            eth_call_no_gas: x
                .eth_call_no_gas
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for EnvVars {
    fn default() -> Self {
        ENV_VARS.clone()
    }
}

#[derive(Clone, Debug, Envconfig)]
struct Inner {
    #[envconfig(from = "GRAPH_GETH_ETH_CALL_ERRORS", default = "")]
    geth_eth_call_errors: String,
    #[envconfig(from = "GRAPH_ETH_GET_LOGS_MAX_CONTRACTS", default = "2000")]
    get_logs_max_contracts: usize,

    #[envconfig(from = "ETHEREUM_TRACE_STREAM_STEP_SIZE", default = "50")]
    trace_stream_step_size: BlockNumber,
    #[envconfig(from = "GRAPH_ETHEREUM_MAX_EVENT_ONLY_RANGE", default = "500")]
    max_event_only_range: BlockNumber,
    #[envconfig(from = "ETHEREUM_BLOCK_BATCH_SIZE", default = "10")]
    block_batch_size: usize,
    #[envconfig(from = "GRAPH_ETHEREUM_MAX_BLOCK_RANGE_SIZE", default = "2000")]
    max_block_range_size: BlockNumber,
    #[envconfig(from = "GRAPH_ETHEREUM_JSON_RPC_TIMEOUT", default = "180")]
    json_rpc_timeout_in_secs: u64,
    #[envconfig(from = "GRAPH_ETHEREUM_REQUEST_RETRIES", default = "10")]
    request_retries: usize,
    #[envconfig(
        from = "GRAPH_ETHEREUM_BLOCK_INGESTOR_MAX_CONCURRENT_JSON_RPC_CALLS_FOR_TXN_RECEIPTS",
        default = "1000"
    )]
    block_ingestor_max_concurrent_json_rpc_calls: usize,
    #[envconfig(from = "GRAPH_ETHEREUM_FETCH_TXN_RECEIPTS_IN_BATCHES")]
    fetch_receipts_in_batches: Option<EnvVarBoolean>,
    #[envconfig(from = "GRAPH_ETHEREUM_CLEANUP_BLOCKS", default = "false")]
    cleanup_blocks: EnvVarBoolean,
    #[envconfig(
        from = "GRAPH_ETHEREUM_TARGET_TRIGGERS_PER_BLOCK_RANGE",
        default = "100"
    )]
    target_triggers_per_block_range: u64,
    #[envconfig(from = "GRAPH_ETHEREUM_GENESIS_BLOCK_NUMBER", default = "0")]
    genesis_block_number: u64,
    #[envconfig(from = "GRAPH_ETH_CALL_NO_GAS", default = "421613")]
    eth_call_no_gas: String,
}
