use fuel_core_types::fuel_crypto::PublicKey;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    /// Maximum of subscriptions to listen to updates of a transaction.
    pub max_tx_update_subscriptions: usize,
    /// Maximum time to keep subscriptions alive.
    pub subscription_ttl: Duration,
    /// Maximum time to keep the status in the cache of the manager.
    pub status_cache_ttl: Duration,
    /// Protocol Signing Key, i.e. the block signer's public key
    pub protocol_public_key: PublicKey,
    /// Enable metrics when set to true
    pub metrics: bool,
}

#[cfg(feature = "test-helpers")]
impl Default for Config {
    fn default() -> Self {
        Self {
            max_tx_update_subscriptions: 1000,
            subscription_ttl: Duration::from_secs(60 * 10),
            status_cache_ttl: Duration::from_secs(5),
            protocol_public_key: PublicKey::default(),
            metrics: false,
        }
    }
}
