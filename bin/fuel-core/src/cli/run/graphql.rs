//! Clap configuration related to GraphQL service.

use std::net;

#[derive(Debug, Clone, clap::Args)]
pub struct GraphQLArgs {
    /// The IP address to bind the GraphQL service to.
    #[clap(long = "ip", default_value = "127.0.0.1", value_parser, env)]
    pub ip: net::IpAddr,

    /// The port to bind the GraphQL service to.
    #[clap(long = "port", default_value = "4000", env)]
    pub port: u16,

    /// The max depth of GraphQL queries.
    #[clap(long = "graphql-max-depth", default_value = "16", env)]
    pub graphql_max_depth: usize,

    /// The max complexity of GraphQL queries.
    #[clap(long = "graphql-max-complexity", default_value = "20000", env)]
    pub graphql_max_complexity: usize,

    /// The max recursive depth of GraphQL queries.
    #[clap(long = "graphql-max-recursive-depth", default_value = "16", env)]
    pub graphql_max_recursive_depth: usize,

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
}
