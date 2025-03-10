//! Clap configuration related to TxStatusManager service.

#[derive(Debug, Clone, clap::Args)]
pub struct TxStatusManagerArgs {
    /// The maximum number of active status subscriptions.
    #[clap(long = "tx-number-active-subscriptions", default_value = "4064", env)]
    pub tx_number_active_subscriptions: usize,
    /// The maximum time to keep the status of the transactions in the cache.
    #[clap(long = "tx-status-manager-cache-ttl", default_value = "5s", env)]
    pub status_cache_ttl: humantime::Duration,
}
