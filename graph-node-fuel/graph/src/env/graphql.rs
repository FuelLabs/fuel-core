use std::fmt;

use super::*;

#[derive(Clone)]
pub struct EnvVarsGraphQl {
    /// Set by the flag `ENABLE_GRAPHQL_VALIDATIONS`. On by default.
    pub enable_validations: bool,
    /// Set by the flag `SILENT_GRAPHQL_VALIDATIONS`. On by default.
    pub silent_graphql_validations: bool,
    /// This is the timeout duration for SQL queries.
    ///
    /// If it is not set, no statement timeout will be enforced. The statement
    /// timeout is local, i.e., can only be used within a transaction and
    /// will be cleared at the end of the transaction.
    ///
    /// Set by the environment variable `GRAPH_SQL_STATEMENT_TIMEOUT` (expressed
    /// in seconds). No default value is provided.
    pub sql_statement_timeout: Option<Duration>,

    /// Set by the environment variable `GRAPH_CACHED_SUBGRAPH_IDS` (comma
    /// separated). When the value of the variable is `*`, queries are cached
    /// for all subgraphs, which is the default
    /// behavior.
    pub cached_subgraph_ids: CachedSubgraphIds,
    /// In how many shards (mutexes) the query block cache is split.
    /// Ideally this should divide 256 so that the distribution of queries to
    /// shards is even.
    ///
    /// Set by the environment variable `GRAPH_QUERY_BLOCK_CACHE_SHARDS`. The
    /// default value is 128.
    pub query_block_cache_shards: u8,
    /// Set by the environment variable `GRAPH_QUERY_LFU_CACHE_SHARDS`. The
    /// default value is set to whatever `GRAPH_QUERY_BLOCK_CACHE_SHARDS` is set
    /// to. Set to 0 to disable this cache.
    pub query_lfu_cache_shards: u8,
    /// How many blocks per network should be kept in the query cache. When the
    /// limit is reached, older blocks are evicted. This should be kept small
    /// since a lookup to the cache is O(n) on this value, and the cache memory
    /// usage also increases with larger number. Set to 0 to disable
    /// the cache.
    ///
    /// Set by the environment variable `GRAPH_QUERY_CACHE_BLOCKS`. The default
    /// value is 2.
    pub query_cache_blocks: usize,
    /// Maximum total memory to be used by the cache. Each block has a max size of
    /// `QUERY_CACHE_MAX_MEM` / (`QUERY_CACHE_BLOCKS` *
    /// `GRAPH_QUERY_BLOCK_CACHE_SHARDS`).
    ///
    /// Set by the environment variable `GRAPH_QUERY_CACHE_MAX_MEM` (expressed
    /// in MB). The default value is 1GB.
    pub query_cache_max_mem: usize,
    /// Set by the environment variable `GRAPH_QUERY_CACHE_STALE_PERIOD`. The
    /// default value is 100.
    pub query_cache_stale_period: u64,
    /// Set by the environment variable `GRAPH_GRAPHQL_QUERY_TIMEOUT` (expressed in
    /// seconds). No default value is provided.
    pub query_timeout: Option<Duration>,
    /// Set by the environment variable `GRAPH_GRAPHQL_MAX_COMPLEXITY`. No
    /// default value is provided.
    pub max_complexity: Option<u64>,
    /// Set by the environment variable `GRAPH_GRAPHQL_MAX_DEPTH`. The default
    /// value is 255.
    pub max_depth: u8,
    /// Set by the environment variable `GRAPH_GRAPHQL_MAX_FIRST`. The default
    /// value is 1000.
    pub max_first: u32,
    /// Set by the environment variable `GRAPH_GRAPHQL_MAX_SKIP`. The default
    /// value is 4294967295 ([`u32::MAX`]).
    pub max_skip: u32,
    /// Allow skipping the check whether a deployment has changed while
    /// we were running a query. Once we are sure that the check mechanism
    /// is reliable, this variable should be removed.
    ///
    /// Set by the flag `GRAPHQL_ALLOW_DEPLOYMENT_CHANGE`. Off by default.
    pub allow_deployment_change: bool,
    /// Set by the environment variable `GRAPH_GRAPHQL_WARN_RESULT_SIZE`. The
    /// default value is [`usize::MAX`].
    pub warn_result_size: usize,
    /// Set by the environment variable `GRAPH_GRAPHQL_ERROR_RESULT_SIZE`. The
    /// default value is [`usize::MAX`].
    pub error_result_size: usize,
    /// Set by the flag `GRAPH_GRAPHQL_MAX_OPERATIONS_PER_CONNECTION`.
    /// Defaults to 1000.
    pub max_operations_per_connection: usize,
    /// Set by the flag `GRAPH_GRAPHQL_DISABLE_BOOL_FILTERS`. Off by default.
    /// Disables AND/OR filters
    pub disable_bool_filters: bool,
    /// Set by the flag `GRAPH_GRAPHQL_DISABLE_CHILD_SORTING`. Off by default.
    /// Disables child-based sorting
    pub disable_child_sorting: bool,
    /// Set by `GRAPH_GRAPHQL_TRACE_TOKEN`, the token to use to enable query
    /// tracing for a GraphQL request. If this is set, requests that have a
    /// header `X-GraphTraceQuery` set to this value will include a trace of
    /// the SQL queries that were run.
    pub query_trace_token: String,
}

// This does not print any values avoid accidentally leaking any sensitive env vars
impl fmt::Debug for EnvVarsGraphQl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "env vars")
    }
}

impl From<InnerGraphQl> for EnvVarsGraphQl {
    fn from(x: InnerGraphQl) -> Self {
        Self {
            enable_validations: x.enable_validations.0,
            silent_graphql_validations: x.silent_graphql_validations.0,
            sql_statement_timeout: x.sql_statement_timeout_in_secs.map(Duration::from_secs),
            cached_subgraph_ids: if x.cached_subgraph_ids == "*" {
                CachedSubgraphIds::All
            } else {
                CachedSubgraphIds::Only(
                    x.cached_subgraph_ids
                        .split(',')
                        .map(str::to_string)
                        .collect(),
                )
            },
            query_block_cache_shards: x.query_block_cache_shards,
            query_lfu_cache_shards: x
                .query_lfu_cache_shards
                .unwrap_or(x.query_block_cache_shards),
            query_cache_blocks: x.query_cache_blocks,
            query_cache_max_mem: x.query_cache_max_mem_in_mb.0 * 1000 * 1000,
            query_cache_stale_period: x.query_cache_stale_period,
            query_timeout: x.query_timeout_in_secs.map(Duration::from_secs),
            max_complexity: x.max_complexity.map(|x| x.0),
            max_depth: x.max_depth.0,
            max_first: x.max_first,
            max_skip: x.max_skip.0,
            allow_deployment_change: x.allow_deployment_change.0,
            warn_result_size: x.warn_result_size.0 .0,
            error_result_size: x.error_result_size.0 .0,
            max_operations_per_connection: x.max_operations_per_connection,
            disable_bool_filters: x.disable_bool_filters.0,
            disable_child_sorting: x.disable_child_sorting.0,
            query_trace_token: x.query_trace_token,
        }
    }
}

#[derive(Clone, Debug, Envconfig)]
pub struct InnerGraphQl {
    #[envconfig(from = "ENABLE_GRAPHQL_VALIDATIONS", default = "false")]
    enable_validations: EnvVarBoolean,
    #[envconfig(from = "SILENT_GRAPHQL_VALIDATIONS", default = "true")]
    silent_graphql_validations: EnvVarBoolean,
    #[envconfig(from = "GRAPH_SQL_STATEMENT_TIMEOUT")]
    sql_statement_timeout_in_secs: Option<u64>,

    #[envconfig(from = "GRAPH_CACHED_SUBGRAPH_IDS", default = "*")]
    cached_subgraph_ids: String,
    #[envconfig(from = "GRAPH_QUERY_BLOCK_CACHE_SHARDS", default = "128")]
    query_block_cache_shards: u8,
    #[envconfig(from = "GRAPH_QUERY_LFU_CACHE_SHARDS")]
    query_lfu_cache_shards: Option<u8>,
    #[envconfig(from = "GRAPH_QUERY_CACHE_BLOCKS", default = "2")]
    query_cache_blocks: usize,
    #[envconfig(from = "GRAPH_QUERY_CACHE_MAX_MEM", default = "1000")]
    query_cache_max_mem_in_mb: NoUnderscores<usize>,
    #[envconfig(from = "GRAPH_QUERY_CACHE_STALE_PERIOD", default = "100")]
    query_cache_stale_period: u64,
    #[envconfig(from = "GRAPH_GRAPHQL_QUERY_TIMEOUT")]
    query_timeout_in_secs: Option<u64>,
    #[envconfig(from = "GRAPH_GRAPHQL_MAX_COMPLEXITY")]
    max_complexity: Option<NoUnderscores<u64>>,
    #[envconfig(from = "GRAPH_GRAPHQL_MAX_DEPTH", default = "")]
    max_depth: WithDefaultUsize<u8, { u8::MAX as usize }>,
    #[envconfig(from = "GRAPH_GRAPHQL_MAX_FIRST", default = "1000")]
    max_first: u32,
    #[envconfig(from = "GRAPH_GRAPHQL_MAX_SKIP", default = "")]
    max_skip: WithDefaultUsize<u32, { u32::MAX as usize }>,
    #[envconfig(from = "GRAPHQL_ALLOW_DEPLOYMENT_CHANGE", default = "false")]
    allow_deployment_change: EnvVarBoolean,
    #[envconfig(from = "GRAPH_GRAPHQL_WARN_RESULT_SIZE", default = "")]
    warn_result_size: WithDefaultUsize<NoUnderscores<usize>, { usize::MAX }>,
    #[envconfig(from = "GRAPH_GRAPHQL_ERROR_RESULT_SIZE", default = "")]
    error_result_size: WithDefaultUsize<NoUnderscores<usize>, { usize::MAX }>,
    #[envconfig(from = "GRAPH_GRAPHQL_MAX_OPERATIONS_PER_CONNECTION", default = "1000")]
    max_operations_per_connection: usize,
    #[envconfig(from = "GRAPH_GRAPHQL_DISABLE_BOOL_FILTERS", default = "false")]
    pub disable_bool_filters: EnvVarBoolean,
    #[envconfig(from = "GRAPH_GRAPHQL_DISABLE_CHILD_SORTING", default = "false")]
    pub disable_child_sorting: EnvVarBoolean,
    #[envconfig(from = "GRAPH_GRAPHQL_TRACE_TOKEN", default = "")]
    query_trace_token: String,
}
