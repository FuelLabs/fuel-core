use std::collections::HashMap;

use async_trait::async_trait;
use fuel_storage::Storage;
use fuel_types::{Address, AssetId, Bytes32, Word};
use tokio::sync::oneshot;

#[cfg_attr(feature = "serde-types", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct DepositCoin {
    pub owner: Address,
    pub amount: Word,
    pub asset_id: AssetId,
    pub deposited_da_height: u64,
    pub fuel_block_spend: Option<u64>,
}

// Database has two main functionalities, ValidatorSet and TokenDeposits.
// From relayer perspective TokenDeposits are just insert when they get finalized.
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
        deposited_da_height: u64, // eth block when deposit is made
        owner: Address,       // owner
        asset_id: AssetId,
        amount: Word,
    ) {
        let coin = DepositCoin {
            owner,
            amount,
            asset_id,
            deposited_da_height,
            fuel_block_spend: None,
        };
        // TODO check what id are we going to use
        // depends on https://github.com/FuelLabs/fuel-specs/issues/106
        let _ = Storage::<Bytes32, DepositCoin>::insert(self,&deposit_nonce,&coin);
    }

    /// Asumption is that validator state is already checked in da side, and we can blidly apply
    /// changed to db without checking if we have enought stake to reduce.
    /// What needs to be done is to have validator set state and diff as separate database values.
    async fn insert_validators_diff(&mut self, da_height: u64, stakes: &HashMap<Address, u64>) {
        let _ = Storage::<u64,HashMap<Address,u64>>::insert(self, &da_height,stakes);
    }

    /// get stakes difference between fuel blocks. Return vector of changed (some blocks are not going to have any change)
    async fn get_validator_diffs(
            &self,
            from_da_height: u64,
            to_da_height: Option<u64>,
    ) -> Vec<(u64,HashMap<Address, u64>)>;

    /// Apply validators diff to validator set and update validators_da_height. This operation needs
    /// to be atomic.
    async fn apply_validator_diffs(&mut self, changes: &HashMap<Address,u64>, da_height: u64) {
        // this is reimplemented inside fuel-core db to assure it is atomic operation in case of poweroff situation
        for ( address, stake) in changes {
            let _ = Storage::<Address,u64>::insert(self,address,stake);
        }
        self.set_validators_da_height(da_height).await;
    }

    /// current best block number
    async fn get_block_height(&self) -> u64;

    /// get validator set for current eth height
    async fn get_validators(&self) -> HashMap<Address,u64>;

    /// Set data availability block height that corresponds to current_validator_set
    async fn set_validators_da_height(&self, block: u64);

    /// Assume it is allways set as initialization of database.
    async fn get_validators_da_height(&self) -> u64;

    /// set finalized da height that represent last block from da layer that got finalized.
    async fn set_finalized_da_height(&self, block: u64);

    /// Assume it is allways set as initialization of database.
    async fn get_finalized_da_height(&self) -> u64;
}

#[derive(Debug)]
pub enum RelayerEvent {
    //expand with https://docs.rs/tokio/0.2.12/tokio/sync/index.html#oneshot-channel
    // so that we return list of validator to consensus.
    GetValidatorSet {
        /// represent validator set for current block and it is on relayer to calculate it with slider in mind.
        da_height: u64,
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
    Stopped,
    #[error("Temp ProviderError")]
    ProviderError,
    #[error("Validator Set not returned, waiting for eth client sync")]
    ValidatorSetEthClientSyncing,
    #[error("Asked for unknown eth block")]
    InitialSyncAskedForUnknownBlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaSyncState {
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
    DaClientNotConnected,
    DaClientIsSyncing,
    DaClientSynced(DaSyncState),
    Stop,
}
