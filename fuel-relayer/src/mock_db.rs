use async_trait::async_trait;

use std::{
    borrow::Cow,
    collections::{
        BTreeMap,
        HashMap,
    },
    sync::{
        Arc,
        Mutex,
    },
};

use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::{
            Address,
            MessageId,
        },
    },
    db::{
        DelegatesIndexes,
        KvStoreError,
        Messages,
        StakingDiffs,
        ValidatorsSet,
    },
    model::{
        BlockHeight,
        ConsensusId,
        DaBlockHeight,
        Message,
        SealedFuelBlock,
        ValidatorId,
        ValidatorStake,
    },
    relayer::{
        RelayerDb,
        StakingDiff,
        ValidatorSet,
    },
};

#[derive(Default)]
pub(crate) struct Data {
    pub messages: HashMap<MessageId, Message>,
    pub validators: HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)>,
    pub delegator_index: HashMap<Address, Vec<DaBlockHeight>>,
    pub staking_diffs: BTreeMap<DaBlockHeight, StakingDiff>,

    pub chain_height: BlockHeight,
    pub sealed_blocks: HashMap<BlockHeight, Arc<SealedFuelBlock>>,
    pub validators_height: DaBlockHeight,
    pub finalized_da_height: DaBlockHeight,
    pub last_committed_finalized_fuel_height: BlockHeight,
}

#[derive(Default)]
pub(crate) struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl MockDb {
    pub fn tap_sealed_blocks_mut<F, R>(&mut self, func: F) -> R
    where
        F: Fn(&mut HashMap<BlockHeight, Arc<SealedFuelBlock>>) -> R,
    {
        func(&mut self.data.lock().unwrap().sealed_blocks)
    }
}

// TODO: Generate `Storage` implementation with macro.

impl StorageInspect<Messages> for MockDb {
    type Error = KvStoreError;

    fn get(&self, key: &MessageId) -> Result<Option<Cow<Message>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, key: &MessageId) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().messages.contains_key(key))
    }
}

impl StorageMutate<Messages> for MockDb {
    fn insert(
        &mut self,
        key: &MessageId,
        value: &Message,
    ) -> Result<Option<Message>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &MessageId) -> Result<Option<Message>, Self::Error> {
        Ok(self.data.lock().unwrap().messages.remove(key))
    }
}

impl StorageInspect<ValidatorsSet> for MockDb {
    type Error = KvStoreError;

    fn get(
        &self,
        key: &ValidatorId,
    ) -> Result<Option<Cow<(ValidatorStake, Option<ConsensusId>)>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .validators
            .get(key)
            .map(|i| Cow::Owned(*i)))
    }

    fn contains_key(&self, key: &ValidatorId) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().validators.contains_key(key))
    }
}

impl StorageMutate<ValidatorsSet> for MockDb {
    fn insert(
        &mut self,
        key: &ValidatorId,
        value: &(ValidatorStake, Option<ConsensusId>),
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, Self::Error> {
        Ok(self.data.lock().unwrap().validators.insert(*key, *value))
    }

    fn remove(
        &mut self,
        key: &ValidatorId,
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, Self::Error> {
        Ok(self.data.lock().unwrap().validators.remove(key))
    }
}

impl StorageInspect<DelegatesIndexes> for MockDb {
    type Error = KvStoreError;

    fn get(&self, key: &Address) -> Result<Option<Cow<Vec<DaBlockHeight>>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .delegator_index
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, key: &Address) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().delegator_index.contains_key(key))
    }
}

impl StorageMutate<DelegatesIndexes> for MockDb {
    fn insert(
        &mut self,
        key: &Address,
        value: &[DaBlockHeight],
    ) -> Result<Option<Vec<DaBlockHeight>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .delegator_index
            .insert(*key, value.into()))
    }

    fn remove(
        &mut self,
        key: &Address,
    ) -> Result<Option<Vec<DaBlockHeight>>, Self::Error> {
        Ok(self.data.lock().unwrap().delegator_index.remove(key))
    }
}

impl StorageInspect<StakingDiffs> for MockDb {
    type Error = KvStoreError;

    fn get(&self, key: &DaBlockHeight) -> Result<Option<Cow<StakingDiff>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .staking_diffs
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, key: &DaBlockHeight) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().staking_diffs.contains_key(key))
    }
}

impl StorageMutate<StakingDiffs> for MockDb {
    fn insert(
        &mut self,
        key: &DaBlockHeight,
        value: &StakingDiff,
    ) -> Result<Option<StakingDiff>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .staking_diffs
            .insert(*key, value.clone()))
    }

    fn remove(
        &mut self,
        key: &DaBlockHeight,
    ) -> Result<Option<StakingDiff>, Self::Error> {
        Ok(self.data.lock().unwrap().staking_diffs.remove(key))
    }
}

#[async_trait]
impl RelayerDb for MockDb {
    async fn get_chain_height(&self) -> BlockHeight {
        self.data.lock().unwrap().chain_height
    }

    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> Option<Arc<SealedFuelBlock>> {
        self.data
            .lock()
            .unwrap()
            .sealed_blocks
            .get(&height)
            .cloned()
    }

    async fn get_validators(&self) -> ValidatorSet {
        self.data.lock().unwrap().validators.clone()
    }

    async fn set_validators_da_height(&self, height: DaBlockHeight) {
        self.data.lock().unwrap().validators_height = height;
    }

    async fn get_validators_da_height(&self) -> DaBlockHeight {
        self.data.lock().unwrap().validators_height
    }

    async fn set_finalized_da_height(&self, height: DaBlockHeight) {
        self.data.lock().unwrap().finalized_da_height = height;
    }

    async fn get_finalized_da_height(&self) -> DaBlockHeight {
        self.data.lock().unwrap().finalized_da_height
    }

    async fn get_last_committed_finalized_fuel_height(&self) -> BlockHeight {
        self.data
            .lock()
            .unwrap()
            .last_committed_finalized_fuel_height
    }

    async fn set_last_committed_finalized_fuel_height(&self, block_height: BlockHeight) {
        self.data
            .lock()
            .unwrap()
            .last_committed_finalized_fuel_height = block_height;
    }

    async fn get_staking_diffs(
        &self,
        from_da_height: DaBlockHeight,
        to_da_height: Option<DaBlockHeight>,
    ) -> Vec<(DaBlockHeight, StakingDiff)> {
        let mut out = Vec::new();
        let diffs = &self.data.lock().unwrap().staking_diffs;
        for (block, diff) in diffs {
            if *block >= from_da_height {
                if let Some(end_block) = to_da_height {
                    if *block > end_block {
                        break
                    }
                }
                out.push((*block, diff.clone()));
            }
        }
        out
    }
}
