pub(crate) mod json;
pub(crate) mod parquet;

use std::{
    fs::File,
    path::PathBuf,
};

pub use json::*;
pub use parquet::*;

use crate::{
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

use super::{
    contract_balance::ContractBalance,
    contract_state::ContractState,
};

#[derive(Debug, PartialEq)]
pub struct Batch<T> {
    pub data: Vec<T>,
    pub group_index: usize,
}

pub trait BatchGenerator<T> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<T>>>;
}

pub trait BatchWriter<T> {
    fn write_batch(&mut self, elements: Vec<T>) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub enum BatchReader {
    JSONReader {
        source: StateConfig,
        batch_size: usize,
    },
    ParquetReader {
        source_dir: PathBuf,
    },
}

impl BatchReader {
    pub fn coin_batches(self) -> Box<dyn BatchGenerator<CoinConfig>> {
        match self {
            BatchReader::JSONReader { source, batch_size } => Box::new(
                JsonBatchReader::<CoinConfig>::from_state(source, batch_size),
            ),
            BatchReader::ParquetReader { source_dir } => {
                let file = File::open(source_dir.join("coins_config.parquet")).unwrap();
                Box::new(ParquetBatchReader::<File, CoinConfig>::new(file).unwrap())
            }
        }
    }

    pub fn message_batches(self) -> Box<dyn BatchGenerator<MessageConfig>> {
        match self {
            BatchReader::JSONReader { source, batch_size } => Box::new(
                JsonBatchReader::<MessageConfig>::from_state(source, batch_size),
            ),
            BatchReader::ParquetReader { source_dir } => {
                let file =
                    File::open(source_dir.join("messages_config.parquet")).unwrap();
                Box::new(ParquetBatchReader::<File, MessageConfig>::new(file).unwrap())
            }
        }
    }

    pub fn contract_batches(self) -> Box<dyn BatchGenerator<ContractConfig>> {
        match self {
            BatchReader::JSONReader { source, batch_size } => Box::new(
                JsonBatchReader::<ContractConfig>::from_state(source, batch_size),
            ),
            BatchReader::ParquetReader { source_dir } => {
                let file =
                    File::open(source_dir.join("contracts_config.parquet")).unwrap();
                Box::new(ParquetBatchReader::<File, ContractConfig>::new(file).unwrap())
            }
        }
    }

    pub fn contract_state_batches(self) -> Box<dyn BatchGenerator<ContractState>> {
        match self {
            BatchReader::JSONReader { source, batch_size } => Box::new(
                JsonBatchReader::<ContractState>::from_state(source, batch_size),
            ),
            BatchReader::ParquetReader { source_dir } => {
                let file = File::open(source_dir.join("contracts_state_config.parquet"))
                    .unwrap();
                Box::new(ParquetBatchReader::<File, ContractState>::new(file).unwrap())
            }
        }
    }

    pub fn contracts_balance_batches(self) -> Box<dyn BatchGenerator<ContractBalance>> {
        match self {
            BatchReader::JSONReader { source, batch_size } => Box::new(
                JsonBatchReader::<ContractBalance>::from_state(source, batch_size),
            ),
            BatchReader::ParquetReader { source_dir } => {
                let file =
                    File::open(source_dir.join("contracts_balance_config.parquet"))
                        .unwrap();
                Box::new(ParquetBatchReader::<File, ContractBalance>::new(file).unwrap())
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use itertools::Itertools;

//     use crate::CoinConfig;

//     use std::iter::repeat_with;

//     use crate::config::codec::BatchWriter;
//     use bytes::Bytes;

//     use crate::config::codec::{Batch, BatchReader};

//     #[cfg(feature = "random")]
//     #[test]
//     fn encodes_and_decodes_coins() {
//         // given

//         use crate::config::codec::parquet::{ParquetBatchReader, ParquetBatchWriter};
//         let coins = repeat_with(|| CoinConfig::random(&mut rand::thread_rng()))
//             .take(100)
//             .collect_vec();

//         let mut writer = ParquetBatchWriter::<_, CoinConfig>::new(
//             vec![],
//             parquet::basic::Compression::UNCOMPRESSED,
//         )
//         .unwrap();

//         // when
//         writer.write_batch(coins.clone()).unwrap();

//         // then
//         let reader =
//             ParquetBatchReader::<_, CoinConfig>::new(Bytes::from(writer.into_inner()))
//                 .unwrap();

//         let decoded_codes = reader
//             .into_iter()
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();

//         assert_eq!(
//             vec![Batch {
//                 data: coins,
//                 group_index: 0
//             }],
//             decoded_codes
//         );
//     }

//     #[cfg(feature = "random")]
//     #[test]
//     fn reads_coins_in_correct_batch_sizes() {
//         use crate::{config::codec::json::chain_state::ChainState, CoinConfig};

//         let state = ChainState::random(100, 100, &mut rand::thread_rng());
//         let reader = JsonBatchReader::from_state(state.clone(), 50);

//         let read_coins = BatchReader::<CoinConfig, _>::batches(reader)
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();

//         assert_eq!(read_coins.len(), 2);
//         assert_eq!(read_coins[0].data, &state.coins[..50]);
//         assert_eq!(read_coins[1].data, &state.coins[50..]);
//     }

//     #[cfg(feature = "random")]
//     #[test]
//     fn reads_messages_in_correct_batch_sizes() {
//         let state = ChainState::random(100, 100, &mut rand::thread_rng());
//         let reader: JsonBatchReader = JsonBatchReader::from_state(state.clone(), 50);

//         let read_messages = BatchReader::<MessageConfig, _>::batches(reader)
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();

//         assert_eq!(read_messages.len(), 2);
//         assert_eq!(read_messages[0].data, &state.messages[..50]);
//         assert_eq!(read_messages[1].data, &state.messages[50..]);
//     }

//     #[cfg(feature = "random")]
//     #[test]
//     fn reads_contracts_in_correct_batch_sizes() {
//         let state = ChainState::random(100, 100, &mut rand::thread_rng());
//         let reader = JsonBatchReader::from_state(state.clone(), 50);

//         let read_contracts = BatchReader::<ContractConfig, _>::batches(reader)
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();

//         assert_eq!(read_contracts.len(), 2);
//         assert_eq!(read_contracts[0].data, &state.contracts[..50]);
//         assert_eq!(read_contracts[1].data, &state.contracts[50..]);
//     }

//     #[cfg(feature = "random")]
//     #[test]
//     fn reads_contract_state_in_expected_batches() {
//         let state = ChainState::random(2, 100, &mut rand::thread_rng());
//         let reader = JsonBatchReader::from_state(state.clone(), 10);

//         let read_state = BatchReader::<ContractState, _>::batches(reader)
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();

//         assert_eq!(read_state.len(), 2);
//         assert_eq!(read_state[0].data, state.contract_state[0]);
//         assert_eq!(read_state[1].data, state.contract_state[1]);
//     }

//     #[cfg(feature = "random")]
//     #[test]
//     fn reads_contract_balance_in_expected_batches() {
//         let state = ChainState::random(2, 100, &mut rand::thread_rng());
//         let reader = JsonBatchReader::from_state(state.clone(), 10);

//         let read_balance = BatchReader::<ContractBalance, _>::batches(reader)
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();

//         assert_eq!(read_balance.len(), 2);
//         assert_eq!(read_balance[0].data, state.contract_balance[0]);
//         assert_eq!(read_balance[1].data, state.contract_balance[1]);
//     }

//     #[cfg(feature = "random")]
//     #[test]
//     fn writes_correctly() {
//         let data = ChainState::random(100, 100, &mut rand::thread_rng());
//         let mut writer = JsonBatchWriter::new();

//         writer.write_batch(data.contracts.clone()).unwrap();
//         writer.write_batch(data.coins.clone()).unwrap();
//         writer.write_batch(data.messages.clone()).unwrap();
//         for batch in data.contract_state.clone() {
//             writer.write_batch(batch).unwrap();
//         }
//         for batch in data.contract_balance.clone() {
//             writer.write_batch(batch).unwrap();
//         }

//         assert_eq!(writer.state(), &data);
//     }
// }
