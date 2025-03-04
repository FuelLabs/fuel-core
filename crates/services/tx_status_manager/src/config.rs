use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    /// Maximum of subscriptions to listen to updates of a transaction.
    pub max_tx_update_subscriptions: usize,
    /// Maximum transaction time to live.
    pub max_txs_ttl: Duration,
}

#[cfg(feature = "test-helpers")]
impl Default for Config {
    fn default() -> Self {
        Self {
            max_tx_update_subscriptions: 1000,
            max_txs_ttl: Duration::from_secs(60 * 10),
        }
    }
}
