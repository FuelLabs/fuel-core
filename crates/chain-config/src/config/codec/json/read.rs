use std::vec::IntoIter;

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

pub(crate) struct JsonBatchReader {
    source: ChainState,
    batch_size: usize,
}

impl JsonBatchReader {
    pub fn from_state(chain_state: ChainState, batch_size: usize) -> Self {
        Self {
            source: chain_state,
            batch_size,
        }
    }

    fn create_batches<Fun, T>(self, transform: Fun) -> Vec<anyhow::Result<Batch<T>>>
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

    fn premade_batches<Fun, T>(self, transform: Fun) -> Vec<anyhow::Result<Batch<T>>>
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
    for JsonBatchReader
{
    fn batch_iter(self) -> IntoIter<anyhow::Result<Batch<CoinConfig>>> {
        self.create_batches(|ctx| ctx.source.coins).into_iter()
    }
}

impl BatchReader<MessageConfig, IntoIter<anyhow::Result<Batch<MessageConfig>>>>
    for JsonBatchReader
{
    fn batch_iter(self) -> IntoIter<anyhow::Result<Batch<MessageConfig>>> {
        self.create_batches(|ctx| ctx.source.messages).into_iter()
    }
}

impl BatchReader<ContractConfig, IntoIter<anyhow::Result<Batch<ContractConfig>>>>
    for JsonBatchReader
{
    fn batch_iter(self) -> IntoIter<anyhow::Result<Batch<ContractConfig>>> {
        self.create_batches(|ctx| ctx.source.contracts).into_iter()
    }
}

impl BatchReader<ContractState, IntoIter<anyhow::Result<Batch<ContractState>>>>
    for JsonBatchReader
{
    fn batch_iter(self) -> IntoIter<anyhow::Result<Batch<ContractState>>> {
        self.premade_batches(|ctx| ctx.source.contract_state)
            .into_iter()
    }
}

impl BatchReader<ContractBalance, IntoIter<anyhow::Result<Batch<ContractBalance>>>>
    for JsonBatchReader
{
    fn batch_iter(self) -> IntoIter<anyhow::Result<Batch<ContractBalance>>> {
        self.premade_batches(|ctx| ctx.source.contract_balance)
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::codec::BatchReader;

    use super::*;

    #[cfg(feature = "random")]
    #[test]
    fn reads_coins_in_correct_batch_sizes() {
        use crate::{config::codec::json::chain_state::ChainState, CoinConfig};

        let state = ChainState::random(100, 100, &mut rand::thread_rng());
        let reader = JsonBatchReader::from_state(state.clone(), 50);

        let read_coins = BatchReader::<CoinConfig, _>::batch_iter(reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(read_coins.len(), 2);
        assert_eq!(read_coins[0].data, &state.coins[..50]);
        assert_eq!(read_coins[1].data, &state.coins[50..]);
    }

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

    #[cfg(feature = "random")]
    #[test]
    fn reads_contracts_in_correct_batch_sizes() {
        let state = ChainState::random(100, 100, &mut rand::thread_rng());
        let reader = JsonBatchReader::from_state(state.clone(), 50);

        let read_contracts = BatchReader::<ContractConfig, _>::batch_iter(reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(read_contracts.len(), 2);
        assert_eq!(read_contracts[0].data, &state.contracts[..50]);
        assert_eq!(read_contracts[1].data, &state.contracts[50..]);
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_contract_state_in_expected_batches() {
        let state = ChainState::random(2, 100, &mut rand::thread_rng());
        let reader = JsonBatchReader::from_state(state.clone(), 10);

        let read_state = BatchReader::<ContractState, _>::batch_iter(reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(read_state.len(), 2);
        assert_eq!(read_state[0].data, state.contract_state[0]);
        assert_eq!(read_state[1].data, state.contract_state[1]);
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_contract_balance_in_expected_batches() {
        let state = ChainState::random(2, 100, &mut rand::thread_rng());
        let reader = JsonBatchReader::from_state(state.clone(), 10);

        let read_balance = BatchReader::<ContractBalance, _>::batch_iter(reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(read_balance.len(), 2);
        assert_eq!(read_balance[0].data, state.contract_balance[0]);
        assert_eq!(read_balance[1].data, state.contract_balance[1]);
    }
}
