use fuel_core_types::{
    blockchain::primitives::SecretKeyWrapper,
    fuel_asm::Word,
    fuel_tx::ConsensusParameters,
    secrecy::Secret,
};
use tokio::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub trigger: Trigger,
    pub block_gas_limit: Word,
    pub signing_key: Option<Secret<SecretKeyWrapper>>,
    pub metrics: bool,
    pub consensus_params: ConsensusParameters,
    pub min_connected_reserved_peers: usize,
    pub time_until_synced: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: Trigger::default(),
            block_gas_limit: 0,
            signing_key: None,
            metrics: false,
            consensus_params: ConsensusParameters::default(),
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
    /// A new block will be produced when the timer runs out.
    /// Set to `max_block_time` when the txpool is empty, otherwise
    /// `min(max_block_time, max_tx_idle_time)`. If it expires,
    /// but minimum block time hasn't expired yet, then the deadline
    /// is set to `last_block_created + min_block_time`.
    /// See https://github.com/FuelLabs/fuel-core/issues/50#issuecomment-1241895887
    /// Requires `min_block_time` <= `max_tx_idle_time` <= `max_block_time`.
    Hybrid {
        /// Minimum time between two blocks, even if there are more txs available
        min_block_time: Duration,
        /// If there are txs available, but not enough for a full block,
        /// this is how long the block is waiting for more txs
        max_tx_idle_time: Duration,
        /// Time after which a new block is produced, even if it's empty
        max_block_time: Duration,
    },
}
