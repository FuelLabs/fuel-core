use std::marker::PhantomData;

use itertools::Itertools;

use crate::{
    config::{
        codec::{Group, GroupResult},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, GroupDecoder, MessageConfig, StateConfig,
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
    ) -> Vec<anyhow::Result<Group<T>>> {
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

impl Iterator for Decoder<CoinConfig> {
    type Item = GroupResult<CoinConfig>;

    fn next(&mut self) -> Option<Self::Item> {
        let group = Self::create_batches(self.batch_size, self.state.coins.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_batch = n;
        self.next()
    }
}

impl Iterator for Decoder<MessageConfig> {
    type Item = GroupResult<MessageConfig>;

    fn next(&mut self) -> Option<Self::Item> {
        let group = Self::create_batches(self.batch_size, self.state.messages.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_batch = n;
        self.next()
    }
}

impl Iterator for Decoder<ContractConfig> {
    type Item = GroupResult<ContractConfig>;

    fn next(&mut self) -> Option<Self::Item> {
        let group = Self::create_batches(self.batch_size, self.state.contracts.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_batch = n;
        self.next()
    }
}

impl Iterator for Decoder<ContractState> {
    type Item = GroupResult<ContractState>;

    fn next(&mut self) -> Option<Self::Item> {
        let group =
            Self::create_batches(self.batch_size, self.state.contract_state.clone())
                .into_iter()
                .nth(self.next_batch);
        self.next_batch += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_batch = n;
        self.next()
    }
}

impl Iterator for Decoder<ContractBalance> {
    type Item = GroupResult<ContractBalance>;

    fn next(&mut self) -> Option<Self::Item> {
        let group =
            Self::create_batches(self.batch_size, self.state.contract_balance.clone())
                .into_iter()
                .nth(self.next_batch);
        self.next_batch += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_batch = n;
        self.next()
    }
}

impl<T> GroupDecoder<T> for Decoder<T> where Decoder<T>: Iterator<Item = GroupResult<T>> {}
