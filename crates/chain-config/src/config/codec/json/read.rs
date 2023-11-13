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
};

use super::chain_state::ChainState;

pub struct JsonBatchReader<T> {
    source: ChainState,
    batch_size: usize,
    data: PhantomData<T>,
    next_batch: usize,
}

impl<T> JsonBatchReader<T> {
    pub fn from_state(chain_state: ChainState, batch_size: usize) -> Self {
        Self {
            source: chain_state,
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
        let a = Self::create_batches(self.batch_size, self.source.coins.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchGenerator<MessageConfig> for JsonBatchReader<MessageConfig> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<MessageConfig>>> {
        let a = Self::create_batches(self.batch_size, self.source.messages.clone())
            .into_iter()
            .nth(self.next_batch);
        self.next_batch += 1;
        a
    }
}

impl BatchGenerator<ContractConfig> for JsonBatchReader<ContractConfig> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<ContractConfig>>> {
        let a = Self::create_batches(self.batch_size, self.source.contracts.clone())
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

#[cfg(test)]
mod tests {
    use crate::config::codec::BatchReader;

    use super::*;
    //     #[cfg(feature = "random")]
    // #[test]
    // fn reads_coins_in_correct_batch_sizes() {
    // use crate::{
    // config::codec::json::chain_state::ChainState,
    // CoinConfig,
    // };
    //
    // let state = ChainState::random(100, 100, &mut rand::thread_rng());
    // let reader = JsonBatchReader::from_state(state.clone(), 50);
    //
    // let read_coins = BatchReader::<CoinConfig, _>::batches(reader)
    // .collect::<Result<Vec<_>, _>>()
    // .unwrap();
    //
    // assert_eq!(read_coins.len(), 2);
    // assert_eq!(read_coins[0].data, &state.coins[..50]);
    // assert_eq!(read_coins[1].data, &state.coins[50..]);
    // }
    //
    // #[cfg(feature = "random")]
    // #[test]
    // fn reads_messages_in_correct_batch_sizes() {
    //     use crate::config::codec::Batch;
    //
    //     let state = ChainState::random(100, 100, &mut rand::thread_rng());
    //     let mut reader: JsonBatchReader<MessageConfig> =
    //         JsonBatchReader::from_state(state.clone(), 50);
    //
    //     let mut read_messages: Vec<Batch<MessageConfig>> = vec![];
    //     while let Ok(Some(batch)) = reader.batch_iter() {
    //         read_messages.push(batch);
    //     }
    //
    //     assert_eq!(read_messages.len(), 2);
    //     assert_eq!(read_messages[0].data, &state.messages[..50]);
    //     assert_eq!(read_messages[1].data, &state.messages[50..]);
    // }
    //
    // #[cfg(feature = "random")]
    // #[test]
    // fn reads_contracts_in_correct_batch_sizes() {
    // let state = ChainState::random(100, 100, &mut rand::thread_rng());
    // let reader = JsonBatchReader::from_state(state.clone(), 50);
    //
    // let read_contracts = BatchReader::<ContractConfig, _>::batches(reader)
    // .collect::<Result<Vec<_>, _>>()
    // .unwrap();
    //
    // assert_eq!(read_contracts.len(), 2);
    // assert_eq!(read_contracts[0].data, &state.contracts[..50]);
    // assert_eq!(read_contracts[1].data, &state.contracts[50..]);
    // }
    //
    // #[cfg(feature = "random")]
    // #[test]
    // fn reads_contract_state_in_expected_batches() {
    // let state = ChainState::random(2, 100, &mut rand::thread_rng());
    // let reader = JsonBatchReader::from_state(state.clone(), 10);
    //
    // let read_state = BatchReader::<ContractState, _>::batches(reader)
    // .collect::<Result<Vec<_>, _>>()
    // .unwrap();
    //
    // assert_eq!(read_state.len(), 2);
    // assert_eq!(read_state[0].data, state.contract_state[0]);
    // assert_eq!(read_state[1].data, state.contract_state[1]);
    // }
    //
    // #[cfg(feature = "random")]
    // #[test]
    // fn reads_contract_balance_in_expected_batches() {
    // let state = ChainState::random(2, 100, &mut rand::thread_rng());
    // let reader = JsonBatchReader::from_state(state.clone(), 10);
    //
    // let read_balance = BatchReader::<ContractBalance, _>::batches(reader)
    // .collect::<Result<Vec<_>, _>>()
    // .unwrap();
    //
    // assert_eq!(read_balance.len(), 2);
    // assert_eq!(read_balance[0].data, state.contract_balance[0]);
    // assert_eq!(read_balance[1].data, state.contract_balance[1]);
    // }
}
