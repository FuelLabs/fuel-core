//! Clap configuration related to TxStatusManager service.

#[derive(Debug, Clone, clap::Args)]
pub struct TxStatusManagerArgs {
    /// The maximum number of active status subscriptions.
    ///
    /// Every `submitAndAwaitStatus` client holds one subscription for the
    /// whole submit->(pre)confirmation wait, so this cap gates transaction
    /// intake directly: measured on the o2 orderbook benchmark, a saturated
    /// node holds ~1 subscription per in-flight transaction. 4064 (the old
    /// default) capped high-frequency submitters well below node capacity.
    #[clap(
        long = "tx-number-active-subscriptions",
        default_value = "16384",
        env
    )]
    pub tx_number_active_subscriptions: usize,
    /// The maximum time to keep the status of the transactions in the cache.
    #[clap(long = "tx-status-manager-cache-ttl", default_value = "5s", env)]
    pub status_cache_ttl: humantime::Duration,
}
