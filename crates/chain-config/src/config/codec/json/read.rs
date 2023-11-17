use std::marker::PhantomData;

use itertools::Itertools;

use crate::{
    config::{
        codec::{Batch, BatchReaderTrait},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::chain_state::ChainState;

pub struct JsonBatchReader<T> {
    state: ChainState,
    batch_size: usize,
    data: PhantomData<T>,
    next_batch: usize,
}

impl<T> JsonBatchReader<T> {
    pub fn new<R: std::io::Read>(reader: R, batch_size: usize) -> Self {
        Self {
            state: serde_json::from_reader(reader).unwrap(),
            batch_size,
            data: PhantomData,
            next_batch: 0,
        }
    }

    pub fn create_batches(
        batch_size: usize,
        items: Vec<T>,
    ) -> Vec<anyhow::Result<Batch<T>>>
    where
        T: Clone,
    {
        items
            .iter()
            .chunks(batch_size)
            .into_iter()
            .map(|chunk| chunk.cloned().collect_vec())
            .enumerate()
            .map(|(index, vec_chunk)| {
                Ok(Batch {
                    data: vec_chunk,
                    group_index: index,
                })
            })
            .collect_vec()
    }
}

impl BatchReaderTrait for JsonBatchReader<CoinConfig> {
    type Item = CoinConfig;
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<Self::Item>>> {
        let a = Self::create_batches(self.batch_size, self.state.coins.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchReaderTrait for JsonBatchReader<MessageConfig> {
    type Item = MessageConfig;
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<Self::Item>>> {
        let a = Self::create_batches(self.batch_size, self.state.messages.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchReaderTrait for JsonBatchReader<ContractConfig> {
    type Item = ContractConfig;
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<Self::Item>>> {
        let a = Self::create_batches(self.batch_size, self.state.contracts.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchReaderTrait for JsonBatchReader<ContractState> {
    type Item = ContractState;
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<Self::Item>>> {
        let a = Self::create_batches(self.batch_size, self.state.contract_state.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchReaderTrait for JsonBatchReader<ContractBalance> {
    type Item = ContractBalance;
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<Self::Item>>> {
        let a =
            Self::create_batches(self.batch_size, self.state.contract_balance.clone())
                .into_iter()
                .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}
