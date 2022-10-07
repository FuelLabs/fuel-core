use crate::{
    common::{
        fuel_storage::{
            StorageAsMut,
            StorageMutate,
        },
        fuel_types::Address,
    },
    db::{
        KvStoreError,
        Messages,
    },
    model::{
        BlockHeight,
        CheckedMessage,
        ConsensusId,
        DaBlockHeight,
        SealedFuelBlock,
        ValidatorId,
        ValidatorStake,
    },
};
use async_trait::async_trait;
use derive_more::{
    Deref,
    DerefMut,
};
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::{
    mpsc,
    oneshot,
};

pub use thiserror::Error;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct StakingDiff {
    /// Validator registration, it is pair of old consensus key and new one, where consensus address
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

// Database has two main functionalities, ValidatorSet and Bridge Message.
// From relayer perspective messages are just inserted when they get finalized.
// But for ValidatorSet, it is little bit different.
#[async_trait]
pub trait RelayerDb: StorageMutate<Messages, Error = KvStoreError> + Send + Sync {
    /// Add bridge message to database. Messages are not revertible.
    async fn insert_message(&mut self, message: &CheckedMessage) {
        let _ = self
            .storage::<Messages>()
            .insert(message.id(), message.as_ref());
    }

    /// current best block number
    async fn get_chain_height(&self) -> BlockHeight;

    async fn get_sealed_block(&self, height: BlockHeight)
        -> Option<Arc<SealedFuelBlock>>;

    /// set finalized da height that represent last block from da layer that got finalized.
    async fn set_finalized_da_height(&self, block: DaBlockHeight);

    /// Assume it is always set as initialization of database.
    async fn get_finalized_da_height(&self) -> Option<DaBlockHeight>;

    /// Get the last fuel block height that was published to the da layer.
    async fn get_last_published_fuel_height(&self) -> Option<BlockHeight>;

    /// Set the last fuel block height that was published to the da layer.
    async fn set_last_published_fuel_height(&self, block_height: BlockHeight);
}

pub type ValidatorSet = HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)>;

#[derive(Debug)]
pub enum RelayerRequest {
    // expand with https://docs.rs/tokio/0.2.12/tokio/sync/index.html#oneshot-channel
    // so that we return list of validator to consensus.
    GetValidatorSet {
        /// represent validator set for current block and it is on relayer to calculate it with slider in mind.
        da_height: DaBlockHeight,
        response: oneshot::Sender<Result<ValidatorSet, RelayerError>>,
    },
    GetStatus {
        response: oneshot::Sender<RelayerStatus>,
    },
    Stop,
}

#[derive(Clone, Deref, DerefMut)]
pub struct Sender(mpsc::Sender<RelayerRequest>);

impl Sender {
    pub fn new(sender: mpsc::Sender<RelayerRequest>) -> Self {
        Self(sender)
    }

    /// Create a dummy sender
    pub fn noop() -> Self {
        let (tx, mut rx) = mpsc::channel(100);

        // drop any messages sent on the channel to avoid backpressure or memory leaks
        tokio::spawn(async move {
            loop {
                // simply drop any received events
                let _ = rx.recv().await;
            }
        });
        Self(tx)
    }

    pub async fn get_validator_set(
        &self,
        da_height: DaBlockHeight,
    ) -> anyhow::Result<ValidatorSet> {
        let (response, receiver) = oneshot::channel();
        let _ = self
            .send(RelayerRequest::GetValidatorSet {
                da_height,
                response,
            })
            .await;
        receiver
            .await
            .map_err(anyhow::Error::from)?
            .map_err(Into::into)
    }

    pub async fn get_status(&self) -> anyhow::Result<RelayerStatus> {
        let (response, receiver) = oneshot::channel();
        let _ = self.send(RelayerRequest::GetStatus { response }).await;
        receiver.await.map_err(Into::into)
    }
}

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
    /// discard all logs from log stream and start receiving new ones.
    OverlappingSync,
    /// We have all past logs ready and can just listen to new ones coming from eth.
    Synced,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RelayerStatus {
    DaClientNotConnected,
    DaClientIsSyncing,
    DaClientSynced(DaSyncState),
    Stop,
}
