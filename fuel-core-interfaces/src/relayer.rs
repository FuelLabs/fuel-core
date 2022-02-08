use std::collections::HashMap;

use async_trait::async_trait;
use fuel_types::{Address, Bytes32, Color, Word};
use tokio::sync::oneshot;

// Database has two main functionalities, ValidatorSet and TokenDeposits.
// From relayer perspectiv TokenDeposits are just insert when they get finalized. 
// But for ValidatorSet, It is litle bit different. 

#[async_trait]
pub trait RelayerDB: Send + Sync {
    /// Asumption is that validator state is already checke in eth side, and we can blidly apply
    /// changed to db without checking if we have enought stake to reduce.
    /// What needs to be done is to have validator set state and diff as saparate database values.
    async fn insert_validator_changes(&self, fuel_block: u64, stakes: &HashMap<Address, u64>);

    /// Return validator set inside HashMap and fuel_block that this validator set is assigned to.
    /// async fn get_latest_validator_set(&self) -> (HashMap<Address,u64>,u64);

    /// get stakes difference between fuel blocks. Return vector of changed (some blocks are not going to have any change)
    /// 
    async fn get_validator_changes(&self, from_fuel_block: u64, to_fuel_block: Option<u64>) -> Vec<(u64,HashMap<Address,u64>)>;

    /// deposit token to database. Token deposits are not revertable
    async fn insert_token_deposit(
        &self,
        deposit_nonce: Bytes32, // this is ID
        fuel_block: u64, //block after it becomes available for using
        account: Address, // owner
        token: Color,
        amount: Word,
    );

    /// set newest finalized eth block
    async fn set_eth_finalized_block(&self, block: u64);

    /// assume it is allways set sa initialization of database.
    async fn get_eth_finalized_block(&self) -> u64;

    /// set last finalized fuel block. In usual case this will be 
    async fn set_fuel_finalized_block(&self, block: u64);
    /// Assume it is allways set as initialization of database.
    async fn get_fuel_finalized_block(&self) -> u64;
}


#[derive(Debug)]
pub enum RelayerEvent {
    NewBlock(u64),
    //expand with https://docs.rs/tokio/0.2.12/tokio/sync/index.html#oneshot-channel
    // so that we return list of validator to consensus.
    GetValidatorSet {
        /// represent validator set for current block and it is on relayer to calculate it with slider in mind.
        fuel_block: u64,
        response_channel: oneshot::Sender<Result<HashMap<Address, u64>, RelayerError>>,
    },
    GetStatus {
        response: oneshot::Sender<RelayerStatus>,
    },
    Stop,
}

pub use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum RelayerError {
    #[error("Temp stopped")]
    Stoped,
    #[error("Temp ProviderError")]
    ProviderError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EthSyncedState {
    /// relayer is syncing old state
    RelayerSyncing,
    /// fetch last N blocks to get their logs. Parse them and save them inside pending state
    /// in parallel start receiving logs from stream and overlap them. when first fetch is finished
    /// discard all logs from log stream and start receiving new onews.
    OverlapingSync,
    /// We have all past logs ready and can just listen to new ones commint from eth
    Synced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayerStatus {
    EthClientNotConnected,
    EthIsSyncing,
    EthSynced(EthSyncedState),
    Stop,
}