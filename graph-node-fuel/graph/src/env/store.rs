use std::fmt;

use crate::bail;

use super::*;

#[derive(Clone)]
pub struct EnvVarsStore {
    /// Set by the environment variable `GRAPH_CHAIN_HEAD_WATCHER_TIMEOUT`
    /// (expressed in seconds). The default value is 30 seconds.
    pub chain_head_watcher_timeout: Duration,
    /// This is how long statistics that influence query execution are cached in
    /// memory before they are reloaded from the database.
    ///
    /// Set by the environment variable `GRAPH_QUERY_STATS_REFRESH_INTERVAL`
    /// (expressed in seconds). The default value is 300 seconds.
    pub query_stats_refresh_interval: Duration,
    /// This can be used to effectively disable the query semaphore by setting
    /// it to a high number, but there's typically no need to configure this.
    ///
    /// Set by the environment variable `GRAPH_EXTRA_QUERY_PERMITS`. The default
    /// value is 0.
    pub extra_query_permits: usize,
    /// Set by the environment variable `LARGE_NOTIFICATION_CLEANUP_INTERVAL`
    /// (expressed in seconds). The default value is 300 seconds.
    pub large_notification_cleanup_interval: Duration,
    /// Set by the environment variable `GRAPH_NOTIFICATION_BROADCAST_TIMEOUT`
    /// (expressed in seconds). The default value is 60 seconds.
    pub notification_broadcast_timeout: Duration,
    /// This variable is only here temporarily until we can settle on the right
    /// batch size through experimentation, and should then just become an
    /// ordinary constant.
    ///
    /// Set by the environment variable `TYPEA_BATCH_SIZE`.
    pub typea_batch_size: usize,
    /// Allows for some optimizations when running relational queries. Set this
    /// to 0 to turn off this optimization.
    ///
    /// Set by the environment variable `TYPED_CHILDREN_SET_SIZE`.
    pub typed_children_set_size: usize,
    /// When enabled, turns `ORDER BY id` into `ORDER BY id, block_range` in
    /// some relational queries.
    ///
    /// Set by the flag `ORDER_BY_BLOCK_RANGE`. Not meant as a user-tunable,
    /// only as an emergency setting for the hosted service. Remove after
    /// 2022-07-01 if hosted service had no issues with it being `true`
    pub order_by_block_range: bool,
    /// Whether to disable the notifications that feed GraphQL
    /// subscriptions. When the flag is set, no updates
    /// about entity changes will be sent to query nodes.
    ///
    /// Set by the flag `GRAPH_DISABLE_SUBSCRIPTION_NOTIFICATIONS`. Not set
    /// by default.
    pub disable_subscription_notifications: bool,
    /// Set by the environment variable `GRAPH_REMOVE_UNUSED_INTERVAL`
    /// (expressed in minutes). The default value is 360 minutes.
    pub remove_unused_interval: chrono::Duration,
    /// Set by the environment variable
    /// `GRAPH_STORE_RECENT_BLOCKS_CACHE_CAPACITY`. The default value is 10 blocks.
    pub recent_blocks_cache_capacity: usize,

    // These should really be set through the configuration file, especially for
    // `GRAPH_STORE_CONNECTION_MIN_IDLE` and
    // `GRAPH_STORE_CONNECTION_IDLE_TIMEOUT`. It's likely that they should be
    // configured differently for each pool.
    /// Set by the environment variable `GRAPH_STORE_CONNECTION_TIMEOUT` (expressed
    /// in milliseconds). The default value is 5000ms.
    pub connection_timeout: Duration,
    /// Set by the environment variable `GRAPH_STORE_CONNECTION_MIN_IDLE`. No
    /// default value is provided.
    pub connection_min_idle: Option<u32>,
    /// Set by the environment variable `GRAPH_STORE_CONNECTION_IDLE_TIMEOUT`
    /// (expressed in seconds). The default value is 600s.
    pub connection_idle_timeout: Duration,

    /// The size of the write queue; this many blocks can be buffered for
    /// writing before calls to transact block operations will block.
    /// Setting this to `0` disables pipelined writes, and writes will be
    /// done synchronously.
    pub write_queue_size: usize,

    /// How long batch operations during copying or grafting should take.
    /// Set by `GRAPH_STORE_BATCH_TARGET_DURATION` (expressed in seconds).
    /// The default is 180s.
    pub batch_target_duration: Duration,

    /// Prune tables where we will remove at least this fraction of entity
    /// versions by rebuilding the table. Set by
    /// `GRAPH_STORE_HISTORY_REBUILD_THRESHOLD`. The default is 0.5
    pub rebuild_threshold: f64,
    /// Prune tables where we will remove at least this fraction of entity
    /// versions, but fewer than `rebuild_threshold`, by deleting. Set by
    /// `GRAPH_STORE_HISTORY_DELETE_THRESHOLD`. The default is 0.05
    pub delete_threshold: f64,
    /// How much history a subgraph with limited history can accumulate
    /// before it will be pruned. Setting this to 1.1 means that the
    /// subgraph will be pruned every time it contains 10% more history (in
    /// blocks) than its history limit. The default value is 1.2 and the
    /// value must be at least 1.01
    pub history_slack_factor: f64,
    /// How long to accumulate changes into a batch before a write has to
    /// happen. Set by the environment variable
    /// `GRAPH_STORE_WRITE_BATCH_DURATION` in seconds. The default is 300s.
    /// Setting this to 0 disables write batching.
    pub write_batch_duration: Duration,
    /// How many changes to accumulate in bytes before a write has to
    /// happen. Set by the environment variable
    /// `GRAPH_STORE_WRITE_BATCH_SIZE`, which is in kilobytes. The default
    /// is 10_000 which corresponds to 10MB. Setting this to 0 disables
    /// write batching.
    pub write_batch_size: usize,
    /// Whether to create GIN indexes for array attributes. Set by
    /// `GRAPH_STORE_CREATE_GIN_INDEXES`. The default is `false`
    pub create_gin_indexes: bool,
    /// Temporary env var in case we need to quickly rollback PR #5010
    pub use_brin_for_all_query_types: bool,
    /// Temporary env var to disable certain lookups in the chain store
    pub disable_block_cache_for_lookup: bool,
}

// This does not print any values avoid accidentally leaking any sensitive env vars
impl fmt::Debug for EnvVarsStore {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "env vars")
    }
}

impl From<InnerStore> for EnvVarsStore {
    fn from(x: InnerStore) -> Self {
        Self {
            chain_head_watcher_timeout: Duration::from_secs(x.chain_head_watcher_timeout_in_secs),
            query_stats_refresh_interval: Duration::from_secs(
                x.query_stats_refresh_interval_in_secs,
            ),
            extra_query_permits: x.extra_query_permits,
            large_notification_cleanup_interval: Duration::from_secs(
                x.large_notification_cleanup_interval_in_secs,
            ),
            notification_broadcast_timeout: Duration::from_secs(
                x.notification_broadcast_timeout_in_secs,
            ),
            typea_batch_size: x.typea_batch_size,
            typed_children_set_size: x.typed_children_set_size,
            order_by_block_range: x.order_by_block_range.0,
            disable_subscription_notifications: x.disable_subscription_notifications.0,
            remove_unused_interval: chrono::Duration::minutes(
                x.remove_unused_interval_in_minutes as i64,
            ),
            recent_blocks_cache_capacity: x.recent_blocks_cache_capacity,
            connection_timeout: Duration::from_millis(x.connection_timeout_in_millis),
            connection_min_idle: x.connection_min_idle,
            connection_idle_timeout: Duration::from_secs(x.connection_idle_timeout_in_secs),
            write_queue_size: x.write_queue_size,
            batch_target_duration: Duration::from_secs(x.batch_target_duration_in_secs),
            rebuild_threshold: x.rebuild_threshold.0,
            delete_threshold: x.delete_threshold.0,
            history_slack_factor: x.history_slack_factor.0,
            write_batch_duration: Duration::from_secs(x.write_batch_duration_in_secs),
            write_batch_size: x.write_batch_size * 1_000,
            create_gin_indexes: x.create_gin_indexes,
            use_brin_for_all_query_types: x.use_brin_for_all_query_types,
            disable_block_cache_for_lookup: x.disable_block_cache_for_lookup,
        }
    }
}

#[derive(Clone, Debug, Envconfig)]
pub struct InnerStore {
    #[envconfig(from = "GRAPH_CHAIN_HEAD_WATCHER_TIMEOUT", default = "30")]
    chain_head_watcher_timeout_in_secs: u64,
    #[envconfig(from = "GRAPH_QUERY_STATS_REFRESH_INTERVAL", default = "300")]
    query_stats_refresh_interval_in_secs: u64,
    #[envconfig(from = "GRAPH_EXTRA_QUERY_PERMITS", default = "0")]
    extra_query_permits: usize,
    #[envconfig(from = "LARGE_NOTIFICATION_CLEANUP_INTERVAL", default = "300")]
    large_notification_cleanup_interval_in_secs: u64,
    #[envconfig(from = "GRAPH_NOTIFICATION_BROADCAST_TIMEOUT", default = "60")]
    notification_broadcast_timeout_in_secs: u64,
    #[envconfig(from = "TYPEA_BATCH_SIZE", default = "150")]
    typea_batch_size: usize,
    #[envconfig(from = "TYPED_CHILDREN_SET_SIZE", default = "150")]
    typed_children_set_size: usize,
    #[envconfig(from = "ORDER_BY_BLOCK_RANGE", default = "true")]
    order_by_block_range: EnvVarBoolean,
    #[envconfig(from = "GRAPH_DISABLE_SUBSCRIPTION_NOTIFICATIONS", default = "false")]
    disable_subscription_notifications: EnvVarBoolean,
    #[envconfig(from = "GRAPH_REMOVE_UNUSED_INTERVAL", default = "360")]
    remove_unused_interval_in_minutes: u64,
    #[envconfig(from = "GRAPH_STORE_RECENT_BLOCKS_CACHE_CAPACITY", default = "10")]
    recent_blocks_cache_capacity: usize,

    // These should really be set through the configuration file, especially for
    // `GRAPH_STORE_CONNECTION_MIN_IDLE` and
    // `GRAPH_STORE_CONNECTION_IDLE_TIMEOUT`. It's likely that they should be
    // configured differently for each pool.
    #[envconfig(from = "GRAPH_STORE_CONNECTION_TIMEOUT", default = "5000")]
    connection_timeout_in_millis: u64,
    #[envconfig(from = "GRAPH_STORE_CONNECTION_MIN_IDLE")]
    connection_min_idle: Option<u32>,
    #[envconfig(from = "GRAPH_STORE_CONNECTION_IDLE_TIMEOUT", default = "600")]
    connection_idle_timeout_in_secs: u64,
    #[envconfig(from = "GRAPH_STORE_WRITE_QUEUE", default = "5")]
    write_queue_size: usize,
    #[envconfig(from = "GRAPH_STORE_BATCH_TARGET_DURATION", default = "180")]
    batch_target_duration_in_secs: u64,
    #[envconfig(from = "GRAPH_STORE_HISTORY_REBUILD_THRESHOLD", default = "0.5")]
    rebuild_threshold: ZeroToOneF64,
    #[envconfig(from = "GRAPH_STORE_HISTORY_DELETE_THRESHOLD", default = "0.05")]
    delete_threshold: ZeroToOneF64,
    #[envconfig(from = "GRAPH_STORE_HISTORY_SLACK_FACTOR", default = "1.2")]
    history_slack_factor: HistorySlackF64,
    #[envconfig(from = "GRAPH_STORE_WRITE_BATCH_DURATION", default = "300")]
    write_batch_duration_in_secs: u64,
    #[envconfig(from = "GRAPH_STORE_WRITE_BATCH_SIZE", default = "10000")]
    write_batch_size: usize,
    #[envconfig(from = "GRAPH_STORE_CREATE_GIN_INDEXES", default = "false")]
    create_gin_indexes: bool,
    #[envconfig(from = "GRAPH_STORE_USE_BRIN_FOR_ALL_QUERY_TYPES", default = "false")]
    use_brin_for_all_query_types: bool,
    #[envconfig(from = "GRAPH_STORE_DISABLE_BLOCK_CACHE_FOR_LOOKUP", default = "false")]
    disable_block_cache_for_lookup: bool,
}

#[derive(Clone, Copy, Debug)]
struct ZeroToOneF64(f64);

impl FromStr for ZeroToOneF64 {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let f = s.parse::<f64>()?;
        if f < 0.0 || f > 1.0 {
            bail!("invalid value: {s} must be between 0 and 1");
        } else {
            Ok(ZeroToOneF64(f))
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct HistorySlackF64(f64);

impl FromStr for HistorySlackF64 {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let f = s.parse::<f64>()?;
        if f < 1.01 {
            bail!("invalid value: {s} must be bigger than 1.01");
        } else {
            Ok(HistorySlackF64(f))
        }
    }
}
