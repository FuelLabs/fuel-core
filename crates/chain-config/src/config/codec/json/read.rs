use std::marker::PhantomData;

use itertools::Itertools;

use crate::{
    config::{
        codec::{
            Batch,
            BatchGenerator,
        },
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

pub struct JsonBatchReader<T> {
    source: StateConfig,
    batch_size: usize,
    data: PhantomData<T>,
    next_batch: usize,
}

impl<T> JsonBatchReader<T> {
    pub fn from_state(state: StateConfig, batch_size: usize) -> Self {
        Self {
            source: state,
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

    fn premade_batches(items: Vec<Vec<T>>) -> Vec<anyhow::Result<Batch<T>>>
    where
        T: Clone,
    {
        items
            .into_iter()
            .enumerate()
            .map(|(index, data)| {
                Ok(Batch {
                    data,
                    group_index: index,
                })
            })
            .collect()
    }
}

impl BatchGenerator<CoinConfig> for JsonBatchReader<CoinConfig> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<CoinConfig>>> {
        let a = Self::create_batches(self.batch_size, self.source.coins.clone().unwrap().clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchGenerator<MessageConfig> for JsonBatchReader<MessageConfig> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<MessageConfig>>> {
        let a = Self::create_batches(self.batch_size, self.source.messages.clone().unwrap().clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchGenerator<ContractConfig> for JsonBatchReader<ContractConfig> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<ContractConfig>>> {
        let a = Self::create_batches(self.batch_size, self.source.contracts.clone().unwrap().clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchGenerator<ContractState> for JsonBatchReader<ContractState> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<ContractState>>> {
        let a = Self::premade_batches(self.source.contract_state.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchGenerator<ContractBalance> for JsonBatchReader<ContractBalance> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<ContractBalance>>> {
        let a = Self::premade_batches(self.source.contract_balance.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}
