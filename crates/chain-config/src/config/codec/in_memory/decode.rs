use std::{
    borrow::Borrow,
    marker::PhantomData,
};

use itertools::Itertools;

use crate::{
    config::{
        codec::{
            Group,
            GroupResult,
        },
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig,
    ContractConfig,
    GroupDecoder,
    MessageConfig,
    StateConfig,
};

pub struct Decoder<R, T> {
    source: R,
    _data_type: PhantomData<T>,
    group_size: usize,
    next_group: usize,
}

impl<R, T> Decoder<R, T> {
    pub fn new(source: R, group_size: usize) -> Self {
        Self {
            source,
            group_size,
            _data_type: PhantomData,
            next_group: 0,
        }
    }

    pub fn group_up(group_size: usize, items: Vec<T>) -> Vec<anyhow::Result<Group<T>>> {
        items
            .into_iter()
            .chunks(group_size)
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

impl<R: Borrow<StateConfig>> Iterator for Decoder<R, CoinConfig> {
    type Item = GroupResult<CoinConfig>;

    fn next(&mut self) -> Option<Self::Item> {
        let group = Self::group_up(self.group_size, self.source.borrow().coins.clone())
            .into_iter()
            .nth(self.next_group);
        self.next_group += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_group = n;
        self.next()
    }
}

impl<R: Borrow<StateConfig>> Iterator for Decoder<R, MessageConfig> {
    type Item = GroupResult<MessageConfig>;

    fn next(&mut self) -> Option<Self::Item> {
        let group =
            Self::group_up(self.group_size, self.source.borrow().messages.clone())
                .into_iter()
                .nth(self.next_group);
        self.next_group += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_group = n;
        self.next()
    }
}

impl<R: Borrow<StateConfig>> Iterator for Decoder<R, ContractConfig> {
    type Item = GroupResult<ContractConfig>;

    fn next(&mut self) -> Option<Self::Item> {
        let group =
            Self::group_up(self.group_size, self.source.borrow().contracts.clone())
                .into_iter()
                .nth(self.next_group);
        self.next_group += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_group = n;
        self.next()
    }
}

impl<R: Borrow<StateConfig>> Iterator for Decoder<R, ContractState> {
    type Item = GroupResult<ContractState>;

    fn next(&mut self) -> Option<Self::Item> {
        let group =
            Self::group_up(self.group_size, self.source.borrow().contract_state.clone())
                .into_iter()
                .nth(self.next_group);
        self.next_group += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_group = n;
        self.next()
    }
}

impl<R> Iterator for Decoder<R, ContractBalance>
where
    R: Borrow<StateConfig>,
{
    type Item = GroupResult<ContractBalance>;

    fn next(&mut self) -> Option<Self::Item> {
        let group = Self::group_up(
            self.group_size,
            self.source.borrow().contract_balance.clone(),
        )
        .into_iter()
        .nth(self.next_group);
        self.next_group += 1;
        group
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_group = n;
        self.next()
    }
}

impl<R, T> GroupDecoder<T> for Decoder<R, T> where
    Decoder<R, T>: Iterator<Item = GroupResult<T>>
{
}
