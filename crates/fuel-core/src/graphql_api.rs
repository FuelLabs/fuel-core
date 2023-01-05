use fuel_core_types::{
    blockchain::primitives::SecretKeyWrapper,
    fuel_tx::ConsensusParameters,
    secrecy::Secret,
};
use std::net::SocketAddr;

pub mod service;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub utxo_validation: bool,
    pub manual_blocks_enabled: bool,
    pub vm_backtrace: bool,
    pub min_gas_price: u64,
    pub max_tx: usize,
    pub max_depth: usize,
    pub transaction_parameters: ConsensusParameters,
    pub consensus_key: Option<Secret<SecretKeyWrapper>>,
}
