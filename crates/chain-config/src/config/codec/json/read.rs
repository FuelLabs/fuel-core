use std::{marker::PhantomData, vec::IntoIter};

use itertools::Itertools;

use crate::{
    config::{
        codec::{Batch, BatchReader},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::chain_state::ChainState;

pub(crate) struct JsonBatchReader<T> {
    source: ChainState,
    batch_size: usize,
    _data: PhantomData<T>,
}

impl<T> JsonBatchReader<T> {
    pub fn from_state(chain_state: ChainState, batch_size: usize) -> Self {
        Self {
            source: chain_state,
            batch_size,
            _data: PhantomData,
        }
    }

    fn create_batches<Fun>(self, transform: Fun) -> Vec<anyhow::Result<Batch<T>>>
    where
        T: Clone,
        Fun: FnOnce(Self) -> Vec<T>,
    {
        let batch_size = self.batch_size;
        transform(self)
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

    fn premade_batches<Fun>(self, transform: Fun) -> Vec<anyhow::Result<Batch<T>>>
    where
        T: Clone,
        Fun: FnOnce(Self) -> Vec<Vec<T>>,
    {
        transform(self)
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

impl BatchReader<CoinConfig, IntoIter<anyhow::Result<Batch<CoinConfig>>>>
    for JsonBatchReader<CoinConfig>
{
    fn batches(self) -> IntoIter<anyhow::Result<Batch<CoinConfig>>> {
        self.create_batches(|ctx| ctx.source.coins).into_iter()
    }
}

impl BatchReader<MessageConfig, IntoIter<anyhow::Result<Batch<MessageConfig>>>>
    for JsonBatchReader<MessageConfig>
{
    fn batches(self) -> IntoIter<anyhow::Result<Batch<MessageConfig>>> {
        self.create_batches(|ctx| ctx.source.messages).into_iter()
    }
}

impl BatchReader<ContractConfig, IntoIter<anyhow::Result<Batch<ContractConfig>>>>
    for JsonBatchReader<ContractConfig>
{
    fn batches(self) -> IntoIter<anyhow::Result<Batch<ContractConfig>>> {
        self.create_batches(|ctx| ctx.source.contracts).into_iter()
    }
}

impl BatchReader<ContractState, IntoIter<anyhow::Result<Batch<ContractState>>>>
    for JsonBatchReader<ContractState>
{
    fn batches(self) -> IntoIter<anyhow::Result<Batch<ContractState>>> {
        self.premade_batches(|ctx| ctx.source.contract_state)
            .into_iter()
    }
}

impl BatchReader<ContractBalance, IntoIter<anyhow::Result<Batch<ContractBalance>>>>
    for JsonBatchReader<ContractBalance>
{
    fn batches(self) -> IntoIter<anyhow::Result<Batch<ContractBalance>>> {
        self.premade_batches(|ctx| ctx.source.contract_balance)
            .into_iter()
    }
}
