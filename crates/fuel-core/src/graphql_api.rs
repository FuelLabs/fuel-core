use fuel_core_storage::{
    Error as StorageError,
    IsNotFound,
};
use fuel_core_types::{
    blockchain::primitives::SecretKeyWrapper,
    fuel_tx::ConsensusParameters,
    fuel_vm::GasCosts,
    secrecy::Secret,
};
use std::net::SocketAddr;

pub mod ports;
#[cfg(feature = "metrics")]
pub(crate) mod prometheus;
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
    pub gas_costs: GasCosts,
    pub consensus_key: Option<Secret<SecretKeyWrapper>>,
}

pub trait IntoApiResult<T> {
    fn into_api_result<NewT, E>(self) -> Result<Option<NewT>, E>
    where
        NewT: From<T>,
        E: From<StorageError>;
}

impl<T> IntoApiResult<T> for Result<T, StorageError> {
    fn into_api_result<NewT, E>(self) -> Result<Option<NewT>, E>
    where
        NewT: From<T>,
        E: From<StorageError>,
    {
        if self.is_not_found() {
            Ok(None)
        } else {
            Ok(Some(self?.into()))
        }
    }
}
