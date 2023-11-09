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
    current_batch_cursor: usize,
}

impl JsonBatchReader {
    pub fn from_state(chain_state: ChainState, batch_size: usize) -> Self {
        Self {
            source: chain_state,
            batch_size,
            current_batch_cursor: 0,
        }
    }

    fn next_batch<Fun, T>(&mut self, transform: Fun) -> Option<Batch<T>>
    where
        T: Clone,
        Fun: for<'a> FnOnce(&'a Self) -> &'a Vec<T>,
    {
        let chunk: Vec<T> = transform(&*self)
            .iter()
            .chunks(self.batch_size)
            .into_iter()
            .nth(self.current_batch_cursor)?
            .cloned()
            .collect_vec();

        let old_batch_cursor = self.current_batch_cursor;
        self.current_batch_cursor += 1;

        Some(Batch {
            data: chunk,
            batch_cursor: old_batch_cursor,
        })
    }
    fn next_premade_batch<Fun, T>(&mut self, transform: Fun) -> Option<Batch<T>>
    where
        T: Clone,
        Fun: for<'a> FnOnce(&'a Self) -> &'a Vec<Vec<T>>,
    {
        let chunk = transform(&*self).get(self.current_batch_cursor).cloned()?;
        let old_batch_cursor = self.current_batch_cursor;
        self.current_batch_cursor += 1;
        Some(Batch {
            data: chunk,
            batch_cursor: old_batch_cursor,
        })
    }
}

impl BatchReader<CoinConfig> for JsonBatchReader {
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<CoinConfig>>> {
        Ok(self.next_batch(|ctx| &ctx.source.coins))
    }
}

impl BatchReader<MessageConfig> for JsonBatchReader {
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<MessageConfig>>> {
        Ok(self.next_batch(|ctx| &ctx.source.messages))
    }
}

impl BatchReader<ContractConfig> for JsonBatchReader {
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<ContractConfig>>> {
        Ok(self.next_batch(|ctx| &ctx.source.contracts))
    }
}

impl BatchReader<ContractState> for JsonBatchReader {
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<ContractState>>> {
        Ok(self.next_premade_batch(|ctx| &ctx.source.contract_state))
    }
}

impl BatchReader<ContractBalance> for JsonBatchReader {
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<ContractBalance>>> {
        Ok(self.next_premade_batch(|ctx| &ctx.source.contract_balance))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::codec::BatchReader;

    use super::*;

    #[cfg(feature = "random")]
    #[test]
    fn reads_coins_in_correct_batch_sizes() {
        use crate::config::codec::{json::chain_state::ChainState, Batch};

        let state = ChainState::random(100, 100, &mut rand::thread_rng());
        let mut reader = JsonBatchReader::from_state(state.clone(), 50);

        let mut read_coins: Vec<Batch<CoinConfig>> = vec![];
        while let Ok(Some(batch)) = reader.read_batch() {
            read_coins.push(batch);
        }

        assert_eq!(read_coins.len(), 2);
        assert_eq!(read_coins[0].data, &state.coins[..50]);
        assert_eq!(read_coins[1].data, &state.coins[50..]);
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_messages_in_correct_batch_sizes() {
        use crate::config::codec::Batch;

        let state = ChainState::random(100, 100, &mut rand::thread_rng());
        let mut reader = JsonBatchReader::from_state(state.clone(), 50);

        let mut read_messages: Vec<Batch<MessageConfig>> = vec![];
        while let Ok(Some(batch)) = reader.read_batch() {
            read_messages.push(batch);
        }

        assert_eq!(read_messages.len(), 2);
        assert_eq!(read_messages[0].data, &state.messages[..50]);
        assert_eq!(read_messages[1].data, &state.messages[50..]);
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_contracts_in_correct_batch_sizes() {
        let state = ChainState::random(100, 100, &mut rand::thread_rng());
        let mut reader = JsonBatchReader::from_state(state.clone(), 50);

        let mut read_contracts: Vec<Batch<ContractConfig>> = vec![];
        while let Ok(Some(batch)) = reader.read_batch() {
            read_contracts.push(batch);
        }

        assert_eq!(read_contracts.len(), 2);
        assert_eq!(read_contracts[0].data, &state.contracts[..50]);
        assert_eq!(read_contracts[1].data, &state.contracts[50..]);
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_contract_state_in_expected_batches() {
        let state = ChainState::random(2, 100, &mut rand::thread_rng());
        let mut reader = JsonBatchReader::from_state(state.clone(), 10);

        let mut read_state: Vec<Batch<ContractState>> = vec![];
        while let Ok(Some(batch)) = reader.read_batch() {
            read_state.push(batch);
        }

        assert_eq!(read_state.len(), 2);
        assert_eq!(read_state[0].data, state.contract_state[0]);
        assert_eq!(read_state[1].data, state.contract_state[1]);
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_contract_balance_in_expected_batches() {
        let state = ChainState::random(2, 100, &mut rand::thread_rng());
        let mut reader = JsonBatchReader::from_state(state.clone(), 10);

        let mut read_balance: Vec<Batch<ContractBalance>> = vec![];
        while let Ok(Some(batch)) = reader.read_batch() {
            read_balance.push(batch);
        }

        assert_eq!(read_balance.len(), 2);
        assert_eq!(read_balance[0].data, state.contract_balance[0]);
        assert_eq!(read_balance[1].data, state.contract_balance[1]);
    }
}
