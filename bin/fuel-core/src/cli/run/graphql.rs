//! Clap configuration related to GraphQL service.

use std::net;

use fuel_core::fuel_core_graphql_api::DEFAULT_QUERY_COSTS;

#[derive(Debug, Clone, clap::Args)]
pub struct GraphQLArgs {
    /// The IP address to bind the GraphQL service to.
    #[clap(long = "ip", default_value = "127.0.0.1", value_parser, env)]
    pub ip: net::IpAddr,

    /// The port to bind the GraphQL service to.
    #[clap(long = "port", default_value = "4000", env)]
    pub port: u16,

    /// The number of threads to use for the GraphQL service.
    #[clap(long = "graphql-number-of-threads", default_value = "2", env)]
    pub graphql_number_of_threads: usize,

    /// The size of the batch fetched from the database by GraphQL service.
    #[clap(long = "graphql-database-batch-size", default_value = "100", env)]
    pub database_batch_size: usize,

    /// The max depth of GraphQL queries.
    #[clap(long = "graphql-max-depth", default_value = "16", env)]
    pub graphql_max_depth: usize,

    /// The max complexity of GraphQL queries.
    #[clap(long = "graphql-max-complexity", default_value = "80000", env)]
    pub graphql_max_complexity: usize,

    /// The max recursive depth of GraphQL queries.
    #[clap(long = "graphql-max-recursive-depth", default_value = "24", env)]
    pub graphql_max_recursive_depth: usize,

    /// The max resolver recursive depth of GraphQL queries.
    #[clap(
        long = "graphql-max-resolver-recursive-depth",
        default_value = "1",
        env
    )]
    pub max_queries_resolver_recursive_depth: usize,

    /// The max number of directives in the query.
    #[clap(long = "graphql-max-directives", default_value = "10", env)]
    pub max_queries_directives: usize,

    /// The max number of concurrent queries.
    #[clap(long = "graphql-max-concurrent-queries", default_value = "1024", env)]
    pub graphql_max_concurrent_queries: usize,

    /// The max body limit of the GraphQL query.
    #[clap(
        long = "graphql-request-body-bytes-limit",
        default_value = "1048576",
        env
    )]
    pub graphql_request_body_bytes_limit: usize,

    /// Time to wait after submitting a query before debug info will be logged about query.
    #[clap(long = "query-log-threshold-time", default_value = "2s", env)]
    pub query_log_threshold_time: humantime::Duration,

    /// Timeout before drop the request.
    #[clap(long = "api-request-timeout", default_value = "30s", env)]
    pub api_request_timeout: humantime::Duration,

    #[clap(flatten)]
    pub costs: QueryCosts,
}

/// Costs for individual graphql queries.
#[derive(Debug, Clone, clap::Args)]
pub struct QueryCosts {
    /// Query costs for getting balances.
    #[clap(
        long = "query-cost-balance-query",
        default_value = DEFAULT_QUERY_COSTS.balance_query.to_string(),
        env
    )]
    pub balance_query: usize,

    /// Query costs for getting coins to spend.
    #[clap(
        long = "query-cost-coins-to-spend", 
        default_value = DEFAULT_QUERY_COSTS.coins_to_spend.to_string(),
        env)]
    pub coins_to_spend: usize,

    /// Query costs for getting peers.
    #[clap(
        long = "query-cost-get-peers",
        default_value = DEFAULT_QUERY_COSTS.get_peers.to_string(),
        env
    )]
    pub get_peers: usize,

    /// Query costs for estimating predicates.
    #[clap(
        long = "query-cost-estimate-predicates",
        default_value = DEFAULT_QUERY_COSTS.estimate_predicates.to_string(),
        env
    )]
    pub estimate_predicates: usize,

    /// Query costs for dry running a set of transactions.
    #[clap(
        long = "query-cost-dry-run",
        default_value = DEFAULT_QUERY_COSTS.dry_run.to_string(),
        env
    )]
    pub dry_run: usize,

    /// Query costs for generating execution trace for a block.®
    #[clap(
        long = "query-cost-storage-read-replay",
        default_value = DEFAULT_QUERY_COSTS.storage_read_replay.to_string(),
        env
    )]
    pub storage_read_replay: usize,

    /// Query costs for submitting a transaction.
    #[clap(
        long = "query-cost-submit",
        default_value = DEFAULT_QUERY_COSTS.submit.to_string(),
        env
    )]
    pub submit: usize,

    /// Query costs for submitting and awaiting a transaction.
    #[clap(
        long = "query-cost-submit-and-await",
        default_value = DEFAULT_QUERY_COSTS.submit_and_await.to_string(),
        env
    )]
    pub submit_and_await: usize,

    /// Query costs for the status change query.
    #[clap(
        long = "query-cost-status-change",
        default_value = DEFAULT_QUERY_COSTS.status_change.to_string(),
        env
    )]
    pub status_change: usize,

    /// Query costs for reading from storage.
    #[clap(
        long = "query-cost-storage-read",
        default_value = DEFAULT_QUERY_COSTS.storage_read.to_string(),
        env
    )]
    pub storage_read: usize,

    /// Query costs for getting a transaction.
    #[clap(
        long = "query-cost-tx-get",
        default_value = DEFAULT_QUERY_COSTS.tx_get.to_string(),
        env
    )]
    pub tx_get: usize,

    /// Query costs for reading tx status.
    #[clap(
        long = "query-cost-tx-status-read",
        default_value = DEFAULT_QUERY_COSTS.tx_status_read.to_string(),
        env
    )]
    pub tx_status_read: usize,

    /// Query costs for getting the raw tx payload.
    #[clap(
        long = "query-cost-tx-raw-payload",
        default_value = DEFAULT_QUERY_COSTS.tx_raw_payload.to_string(),
        env
    )]
    pub tx_raw_payload: usize,

    /// Query costs for block header.
    #[clap(
        long = "query-cost-block-header",
        default_value = DEFAULT_QUERY_COSTS.block_header.to_string(),
        env
    )]
    pub block_header: usize,

    /// Query costs for block transactions.
    #[clap(
        long = "query-cost-block-transactions",
        default_value = DEFAULT_QUERY_COSTS.block_transactions.to_string(),
        env
    )]
    pub block_transactions: usize,

    /// Query costs for block transactions ids.
    #[clap(
        long = "query-cost-block-transactions-ids",
        default_value = DEFAULT_QUERY_COSTS.block_transactions_ids.to_string(),
        env
    )]
    pub block_transactions_ids: usize,

    /// Query costs for iterating over storage entries.
    #[clap(
        long = "query-cost-storage-iterator",
        default_value = DEFAULT_QUERY_COSTS.storage_iterator.to_string(),
        env
    )]
    pub storage_iterator: usize,

    /// Query costs for reading bytecode.
    #[clap(
        long = "query-cost-bytecode-read",
        default_value = DEFAULT_QUERY_COSTS.bytecode_read.to_string(),
        env
    )]
    pub bytecode_read: usize,

    /// Query costs for reading state transition bytecode.
    #[clap(
        long = "query-cost-state-transition-bytecode-read",
        default_value = DEFAULT_QUERY_COSTS.state_transition_bytecode_read.to_string(),
        env
    )]
    pub state_transition_bytecode_read: usize,

    /// Query costs for reading a DA compressed block.
    #[clap(
        long = "query-cost-da-compressed-block-read",
        default_value = DEFAULT_QUERY_COSTS.da_compressed_block_read.to_string(),
        env
    )]
    pub da_compressed_block_read: usize,
}
