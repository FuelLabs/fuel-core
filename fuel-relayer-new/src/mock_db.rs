#![allow(missing_docs)]

use async_trait::async_trait;

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_tx::{
            Address,
            MessageId,
        },
    },
    db::KvStoreError,
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
pub struct Data {
    pub messages: HashMap<MessageId, Message>,

    pub chain_height: BlockHeight,
    pub sealed_blocks: HashMap<BlockHeight, Arc<SealedFuelBlock>>,
    pub finalized_da_height: DaBlockHeight,
    pub last_committed_finalized_fuel_height: BlockHeight,
    pub pending_committed_fuel_height: Option<BlockHeight>,
}

#[derive(Default, Clone)]
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl Storage<MessageId, Message> for MockDb {
    type Error = KvStoreError;

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

#[allow(unused_variables)]
impl Storage<ValidatorId, (ValidatorStake, Option<ConsensusId>)> for MockDb {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &ValidatorId,
        value: &(ValidatorStake, Option<ConsensusId>),
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, Self::Error> {
        todo!()
    }

    fn remove(
        &mut self,
        key: &ValidatorId,
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, Self::Error> {
        todo!()
    }

    fn get<'a>(
        &'a self,
        key: &ValidatorId,
    ) -> Result<Option<Cow<'a, (ValidatorStake, Option<ConsensusId>)>>, Self::Error> {
        todo!()
    }

    fn contains_key(&self, key: &ValidatorId) -> Result<bool, Self::Error> {
        todo!()
    }
}

#[allow(unused_variables)]
impl Storage<Address, Vec<DaBlockHeight>> for MockDb {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Address,
        value: &Vec<DaBlockHeight>,
    ) -> Result<Option<Vec<DaBlockHeight>>, Self::Error> {
        todo!()
    }

    fn remove(
        &mut self,
        key: &Address,
    ) -> Result<Option<Vec<DaBlockHeight>>, Self::Error> {
        todo!()
    }

    fn get<'a>(
        &'a self,
        key: &Address,
    ) -> Result<Option<Cow<'a, Vec<DaBlockHeight>>>, Self::Error> {
        todo!()
    }

    fn contains_key(&self, key: &Address) -> Result<bool, Self::Error> {
        todo!()
    }
}

#[allow(unused_variables)]
impl Storage<DaBlockHeight, StakingDiff> for MockDb {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &DaBlockHeight,
        value: &StakingDiff,
    ) -> Result<Option<StakingDiff>, Self::Error> {
        todo!()
    }

    fn remove(
        &mut self,
        key: &DaBlockHeight,
    ) -> Result<Option<StakingDiff>, Self::Error> {
        todo!()
    }

    fn get<'a>(
        &'a self,
        key: &DaBlockHeight,
    ) -> Result<Option<Cow<'a, StakingDiff>>, Self::Error> {
        todo!()
    }

    fn contains_key(&self, key: &DaBlockHeight) -> Result<bool, Self::Error> {
        todo!()
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

    async fn get_pending_committed_fuel_height(&self) -> Option<BlockHeight> {
        self.data.lock().unwrap().pending_committed_fuel_height
    }

    async fn set_pending_committed_fuel_height(&self, block_height: Option<BlockHeight>) {
        self.data.lock().unwrap().pending_committed_fuel_height = block_height;
    }

    // TODO: Remove
    async fn get_validators(&self) -> ValidatorSet {
        todo!()
    }

    // TODO: Remove
    async fn set_validators_da_height(&self, _height: DaBlockHeight) {
        todo!()
    }

    // TODO: Remove
    async fn get_validators_da_height(&self) -> DaBlockHeight {
        todo!()
    }

    // TODO: Remove
    async fn get_staking_diffs(
        &self,
        _from_da_height: DaBlockHeight,
        _to_da_height: Option<DaBlockHeight>,
    ) -> Vec<(DaBlockHeight, StakingDiff)> {
        todo!()
    }
}
