use fuel_core_types::{
    blockchain::primitives::SecretKeyWrapper,
    fuel_asm::Word,
    secrecy::Secret,
};
use tokio::time::Duration;

#[derive(Default, Debug, Clone)]
pub struct Config {
    pub trigger: Trigger,
    pub block_gas_limit: Word,
    pub signing_key: Option<Secret<SecretKeyWrapper>>,
    pub metrics: bool,
}

/// Block production trigger for PoA operation
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trigger {
    /// See [`fuel_core_chain_config::PoABlockProduction::Instant`].
    #[default]
    Instant,
    /// This node doesn't produce new blocks. Used for passive listener nodes.
    Never,
    /// See [`fuel_core_chain_config::PoABlockProduction::Interval`].
    Interval { block_time: Duration },
    /// See [`fuel_core_chain_config::PoABlockProduction::Hybrid`].
    Hybrid {
        min_block_time: Duration,
        max_tx_idle_time: Duration,
        max_block_time: Duration,
    },
}
