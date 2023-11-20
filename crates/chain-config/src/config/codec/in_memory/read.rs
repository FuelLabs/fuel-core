use std::marker::PhantomData;

use itertools::Itertools;

use crate::{
    config::{
        codec::{Group, GroupDecoder, GroupResult},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig, StateConfig,
};

pub struct Decoder<T> {
    state: StateConfig,
    _data_type: PhantomData<T>,
    batch_size: usize,
    next_batch: usize,
}

impl<T> Decoder<T> {
    pub fn new(state: StateConfig, batch_size: usize) -> Self {
        Self {
            state,
            batch_size,
            _data_type: PhantomData,
            next_batch: 0,
        }
    }

    pub fn create_batches(
        batch_size: usize,
        items: Vec<T>,
    ) -> Vec<anyhow::Result<Group<T>>>
    where
        T: Clone,
    {
        items
            .into_iter()
            .chunks(batch_size)
            .into_iter()
            .map(Itertools::collect_vec)
            .enumerate()
            .map(|(index, vec_chunk)| {
                Ok(Group {
                    data: vec_chunk,
                    index,
                })
            })
            .collect_vec()
    }
}

impl GroupDecoder for Decoder<CoinConfig> {
    type GroupItem = CoinConfig;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.coins.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }

    fn nth_group(&mut self, n: usize) -> Option<GroupResult<Self::GroupItem>> {
        self.next_batch = n;
        self.next_group()
    }
}

impl GroupDecoder for Decoder<MessageConfig> {
    type GroupItem = MessageConfig;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.messages.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }

    fn nth_group(&mut self, n: usize) -> Option<GroupResult<Self::GroupItem>> {
        self.next_batch = n;
        self.next_group()
    }
}

impl GroupDecoder for Decoder<ContractConfig> {
    type GroupItem = ContractConfig;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.contracts.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }

    fn nth_group(&mut self, n: usize) -> Option<GroupResult<Self::GroupItem>> {
        self.next_batch = n;
        self.next_group()
    }
}

impl GroupDecoder for Decoder<ContractState> {
    type GroupItem = ContractState;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.contract_state.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }

    fn nth_group(&mut self, n: usize) -> Option<GroupResult<Self::GroupItem>> {
        self.next_batch = n;
        self.next_group()
    }
}

impl GroupDecoder for Decoder<ContractBalance> {
    type GroupItem = ContractBalance;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a =
            Self::create_batches(self.batch_size, self.state.contract_balance.clone())
                .into_iter()
                .nth(self.next_batch);
        self.next_batch += 1;
        a
    }

    fn nth_group(&mut self, n: usize) -> Option<GroupResult<Self::GroupItem>> {
        self.next_batch = n;
        self.next_group()
    }
}
