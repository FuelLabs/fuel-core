use tokio::time::Duration;

use crate::signer::SignMode;

#[derive(Debug, Clone)]
pub struct Config {
    pub trigger: Trigger,
    pub signer: SignMode,
    pub metrics: bool,
    pub min_connected_reserved_peers: usize,
    pub time_until_synced: Duration,
}

#[cfg(feature = "test-helpers")]
impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: Trigger::default(),
            signer: SignMode::Unavailable,
            metrics: false,
            min_connected_reserved_peers: 0,
            time_until_synced: Duration::ZERO,
        }
    }
}

/// Block production trigger for PoA operation
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// A new block is produced instantly when transactions are available.
    /// This is useful for some test cases.
    #[default]
    Instant,
    /// This node doesn't produce new blocks. Used for passive listener nodes.
    Never,
    /// A new block is produced periodically. Used to simulate consensus block delay.
    Interval { block_time: Duration },
}
