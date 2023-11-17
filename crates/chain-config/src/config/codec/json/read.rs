use std::marker::PhantomData;

use itertools::Itertools;

use crate::{
    config::{
        codec::{Group, GroupDecoder},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::chain_state::ChainState;

pub struct JsonDecoder<T> {
    state: ChainState,
    data: PhantomData<T>,
    batch_size: usize,
    next_batch: usize,
}

impl<T> JsonDecoder<T> {
    pub fn new<R: std::io::Read>(
        mut reader: R,
        batch_size: usize,
    ) -> anyhow::Result<Self> {
        // This is a workaround until the Deserialize implementation is fixed to not require a
        // borrowed string over in fuel-vm.
        let mut contents = String::new();
        reader.read_to_string(&mut contents)?;

        Ok(Self {
            state: serde_json::from_str(&contents)?,
            batch_size,
            data: PhantomData,
            next_batch: 0,
        })
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

impl GroupDecoder for JsonDecoder<CoinConfig> {
    type GroupItem = CoinConfig;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.coins.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl GroupDecoder for JsonDecoder<MessageConfig> {
    type GroupItem = MessageConfig;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.messages.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl GroupDecoder for JsonDecoder<ContractConfig> {
    type GroupItem = ContractConfig;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.contracts.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl GroupDecoder for JsonDecoder<ContractState> {
    type GroupItem = ContractState;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a = Self::create_batches(self.batch_size, self.state.contract_state.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl GroupDecoder for JsonDecoder<ContractBalance> {
    type GroupItem = ContractBalance;
    fn next_group(&mut self) -> Option<anyhow::Result<Group<Self::GroupItem>>> {
        let a =
            Self::create_batches(self.batch_size, self.state.contract_balance.clone())
                .into_iter()
                .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}
