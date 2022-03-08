use std::collections::HashMap;

use async_trait::async_trait;
use fuel_storage::Storage;
use fuel_types::{Address, AssetId, Bytes32, Word};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositCoin {
    pub owner: Address,
    pub amount: Word,
    pub asset_id: AssetId,
    pub eth_block_deposited: u64,
    pub fuel_block_spend: u64,
}

// Database has two main functionalities, ValidatorSet and TokenDeposits.
// From relayer perspectiv TokenDeposits are just insert when they get finalized.
// But for ValidatorSet, It is litle bit different.
#[async_trait]
pub trait RelayerDb:
     Storage<Bytes32, DepositCoin, Error = KvStoreError> // token deposit
    + Storage<Address, u64,Error = KvStoreError> // validator set
    + Storage<u64, HashMap<Address, u64>,Error = KvStoreError> //validator set diff
    + Send
    + Sync
{
    /// deposit token to database. Token deposits are not revertable
    async fn insert_token_deposit(
        &mut self,
        deposit_nonce: Bytes32, // this is ID
        fuel_block: u64,        //block after it becomes available for using
        owner: Address,       // owner
        asset_id: AssetId,
        amount: Word,
    ) {
        let coin = DepositCoin {
            owner,
            amount,
            asset_id,
            eth_block_deposited: fuel_block,
            fuel_block_spend: 0,
        };
        let _ = Storage::<Bytes32, DepositCoin>::insert(self,&deposit_nonce,&coin);
    }

    /// Asumption is that validator state is already checked in eth side, and we can blidly apply
    /// changed to db without checking if we have enought stake to reduce.
    /// What needs to be done is to have validator set state and diff as saparate database values.
    async fn insert_validator_set_diff(&mut self, eth_height: u64, stakes: &HashMap<Address, u64>) {
        let _ = Storage::<u64,HashMap<Address,u64>>::insert(self, &eth_height,stakes);
    }

    async fn apply_current_validator_set(&mut self, changes: HashMap<Address,u64>) {
        for (ref address,ref stake) in changes {
            let _ = Storage::<Address,u64>::insert(self,address,stake);
        }
    }

    /// get validator set for current fuel block
    async fn current_validator_set(&self) -> HashMap<Address,u64>;

    /// set last finalized fuel block. In usual case this will be
    async fn set_current_validator_set_block(&self, block: u64);
    /// Assume it is allways set as initialization of database.
    async fn get_current_validator_set_eth_height(&self) -> u64;

    /// get stakes difference between fuel blocks. Return vector of changed (some blocks are not going to have any change)
    async fn get_validator_set_diff(
            &self,
            from_fuel_block: u64,
            to_fuel_block: Option<u64>,
    ) -> Vec<(u64,HashMap<Address, u64>)>;

    /// current best block number
    async fn get_block_height(&self) -> u64;


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
    //expand with https://docs.rs/tokio/0.2.12/tokio/sync/index.html#oneshot-channel
    // so that we return list of validator to consensus.
    GetValidatorSet {
        /// represent validator set for current block and it is on relayer to calculate it with slider in mind.
        eth_height: u64,
        response_channel: oneshot::Sender<Result<HashMap<Address, u64>, RelayerError>>,
    },
    GetStatus {
        response: oneshot::Sender<RelayerStatus>,
    },
    Stop,
}

pub use thiserror::Error;

use crate::db::KvStoreError;

#[derive(Error, Debug, PartialEq, Eq, Copy, Clone)]
pub enum RelayerError {
    #[error("Temp stopped")]
    Stoped,
    #[error("Temp ProviderError")]
    ProviderError,
    #[error("Validator Set not returned, waiting for eth client sync")]
    ValidatorSetEthClientSyncing,
    #[error("Asked for unknown eth block")]
    InitialSyncAskedForUnknownBlock,
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RelayerStatus {
    EthClientNotConnected,
    EthIsSyncing,
    EthSynced(EthSyncedState),
    Stop,
}
