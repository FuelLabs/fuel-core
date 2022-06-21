use async_trait::async_trait;
use fuel_storage::Storage;
use fuel_types::{Address, Bytes32};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::oneshot;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct StakingDiff {
    /// Validator registration, it is pair of old consensu key and new one, where consensus address
    /// if registered is Some or None if unregistration happened.
    pub validators: HashMap<ValidatorId, ValidatorDiff>,
    /// Register changes for all delegations inside one da block.
    pub delegations: HashMap<Address, Option<HashMap<ValidatorId, ValidatorStake>>>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct ValidatorDiff {
    /// Previous consensus key, None if validator was not set.
    pub previous_consensus_key: Option<ConsensusId>,
    /// New consensus key or None if unregistration happened.
    pub new_consensus_key: Option<ConsensusId>,
}

impl StakingDiff {
    pub fn new(
        validators: HashMap<ValidatorId, ValidatorDiff>,
        delegations: HashMap<Address, Option<HashMap<ValidatorId, ValidatorStake>>>,
    ) -> Self {
        Self {
            validators,
            delegations,
        }
    }
}

// Database has two main functionalities, ValidatorSet and TokenDeposits.
// From relayer perspective TokenDeposits are just insert when they get finalized.
// But for ValidatorSet, It is litle bit different.
#[async_trait]
pub trait RelayerDb:
     Storage<Bytes32, DepositCoin, Error = KvStoreError> // token deposit
    + Storage<ValidatorId, (ValidatorStake, Option<ConsensusId>), Error = KvStoreError> // validator set
    + Storage<Address, Vec<DaBlockHeight>,Error = KvStoreError> // delegate index
    + Storage<DaBlockHeight, StakingDiff, Error = KvStoreError> // staking diff
    + Send
    + Sync
{

    /// deposit token to database. Token deposits are not revertable
    async fn insert_coin_deposit(
        &mut self,
        deposit: DepositCoin,
    ) {
        let _ = Storage::<Bytes32, DepositCoin>::insert(self,&deposit.id(),&deposit);
    }

    /// Insert difference make on staking in this particular DA height.
    async fn insert_staking_diff(&mut self, da_height: DaBlockHeight, stakes: &StakingDiff) {
        let _ = Storage::<DaBlockHeight,StakingDiff>::insert(self, &da_height,stakes);
    }

    /// Query delegate index to find list of blocks that delegation changed
    /// iterate over list of indexed to find height that is less but closest to da_height
    /// Query that block StakeDiff to find actual delegation change.
    async fn get_first_lesser_delegation(&mut self,delegate: &Address, da_height: DaBlockHeight) ->  Option<HashMap<ValidatorId,ValidatorStake>> {
        // get delegate index
        let delegate_index = Storage::<Address,Vec<DaBlockHeight>>::get(self,delegate).expect("Expect to get data without problem")?;
        let mut last_da_height = 0;
        for index in delegate_index.iter() {
            if  *index >= da_height {
                break;
            }
            last_da_height = *index;
        }
        // means that first delegate is in future or not existing in current delegate_index
        if last_da_height == 0 {
            return None
        }
        // get staking diff
        let staking_diff = Storage::<DaBlockHeight,StakingDiff>::get(self, &last_da_height).expect("Expect to get data without problem")?;

        staking_diff.delegations.get(delegate).unwrap().clone()
    }

    async fn append_delegate_index(&mut self, delegate: &Address, da_height: DaBlockHeight) {
        let new_indexes = if let Some(indexes) = Storage::<Address,Vec<DaBlockHeight>>::get(self,delegate).unwrap() {
            let mut indexes = (*indexes).clone();
            indexes.push(da_height);
            indexes
        } else {
            vec![da_height]
        };
        Storage::<Address,Vec<DaBlockHeight>>::insert(self,delegate,&new_indexes).expect("Expect to insert without problem");
    }

    /// get stakes difference between fuel blocks. Return vector of changed (some blocks are not going to have any change)
    async fn get_staking_diffs(
            &self,
            _from_da_height: DaBlockHeight,
            _to_da_height: Option<DaBlockHeight>,
    ) -> Vec<(DaBlockHeight, StakingDiff)> {
        Vec::new()
    }

    /// Apply validators diff to validator set and update validators_da_height. This operation needs
    /// to be atomic.
    async fn apply_validator_diffs(&mut self, da_height: DaBlockHeight, changes: &HashMap<ValidatorId,(ValidatorStake,Option<ConsensusId>)>) {
        // this is reimplemented inside fuel-core db to assure it is atomic operation in case of poweroff situation
        for ( address, stake) in changes {
            let _ = Storage::<ValidatorId,(ValidatorStake,Option<ConsensusId>)>::insert(self,address,stake);
        }
        self.set_validators_da_height(da_height).await;
    }

    /// current best block number
    async fn get_chain_height(&self) -> BlockHeight;

    async fn get_sealed_block(&self, height: BlockHeight) -> Option<Arc<SealedFuelBlock>>;

    /// get validator set for current eth height
    async fn get_validators(&self) -> ValidatorSet;

    /// Set data availability block height that corresponds to current_validator_set
    async fn set_validators_da_height(&self, block: DaBlockHeight);

    /// Assume it is allways set as initialization of database.
    async fn get_validators_da_height(&self) -> DaBlockHeight;

    /// set finalized da height that represent last block from da layer that got finalized.
    async fn set_finalized_da_height(&self, block: DaBlockHeight);

    /// Assume it is allways set as initialization of database.
    async fn get_finalized_da_height(&self) -> DaBlockHeight;

    /// Until blocks gets commited to da layer it is expected for it to still contains consensus
    /// votes and be saved in database until commitment is send to da layer and finalization pariod passes.
    /// In case that commited_finalized_fuel_height is zero we need to return genesis block.
    async fn get_last_commited_finalized_fuel_height(&self) -> BlockHeight;

    /// Set last commited finalized fuel height this means we are safe to remove consensus votes from db
    /// as from this moment they are not needed any more 
    async fn set_last_commited_finalized_fuel_height(&self, block_height: BlockHeight);
}

pub type ValidatorSet = HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)>;

#[derive(Debug)]
pub enum RelayerEvent {
    //expand with https://docs.rs/tokio/0.2.12/tokio/sync/index.html#oneshot-channel
    // so that we return list of validator to consensus.
    GetValidatorSet {
        /// represent validator set for current block and it is on relayer to calculate it with slider in mind.
        da_height: DaBlockHeight,
        response_channel: oneshot::Sender<Result<ValidatorSet, RelayerError>>,
    },
    GetStatus {
        response: oneshot::Sender<RelayerStatus>,
    },
    Stop,
}

pub use thiserror::Error;

use crate::{
    db::KvStoreError,
    model::{
        BlockHeight, ConsensusId, DaBlockHeight, DepositCoin, SealedFuelBlock, ValidatorId,
        ValidatorStake,
    },
};

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
