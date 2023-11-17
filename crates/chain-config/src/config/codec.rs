pub(crate) mod json;
mod moje_nesto;
pub(crate) mod parquet;

use std::{fs::File, path::PathBuf};

pub use json::*;
pub use parquet::*;

use crate::{CoinConfig, ContractConfig, MessageConfig};

use super::{contract_balance::ContractBalance, contract_state::ContractState};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Batch<T> {
    pub data: Vec<T>,
    pub group_index: usize,
}

pub trait BatchReaderTrait {
    type Item;
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<Self::Item>>>;
}

pub trait BatchWriterTrait {
    fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()>;
    fn write_contracts(&mut self, elements: Vec<ContractConfig>) -> anyhow::Result<()>;
    fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()>;
    fn write_contract_state(
        &mut self,
        elements: Vec<ContractState>,
    ) -> anyhow::Result<()>;
    fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()>;
    fn close(self: Box<Self>) -> anyhow::Result<()>;
}

// pub struct BatchReader<T, I: IntoIterator<Item = anyhow::Result<Batch<T>>>> {}
// pub enum BatchReader {
//     JSONReader {
//         source: ChainState,
//         batch_size: usize,
//     },
//     ParquetReader {
//         source_dir: PathBuf,
//     },
// }

// impl BatchReader {
//     pub fn coin_batches(self) -> Box<dyn BatchReaderTrait<CoinConfig>> {
//         match self {
//             BatchReader::JSONReader { source, batch_size } => Box::new(
//                 JsonBatchReader::<CoinConfig>::from_state(source, batch_size),
//             ),
//             BatchReader::ParquetReader { source_dir } => {
//                 let file = File::open(source_dir.join("coins_config.parquet")).unwrap();
//                 Box::new(ParquetBatchReader::<File, CoinConfig>::new(file).unwrap())
//             }
//         }
//     }
//
//     pub fn message_batches(self) -> Box<dyn BatchReaderTrait<MessageConfig>> {
//         match self {
//             BatchReader::JSONReader { source, batch_size } => Box::new(
//                 JsonBatchReader::<MessageConfig>::from_state(source, batch_size),
//             ),
//             BatchReader::ParquetReader { source_dir } => {
//                 let file =
//                     File::open(source_dir.join("messages_config.parquet")).unwrap();
//                 Box::new(ParquetBatchReader::<File, MessageConfig>::new(file).unwrap())
//             }
//         }
//     }
//
//     pub fn contract_batches(self) -> Box<dyn BatchReaderTrait<ContractConfig>> {
//         match self {
//             BatchReader::JSONReader { source, batch_size } => Box::new(
//                 JsonBatchReader::<ContractConfig>::from_state(source, batch_size),
//             ),
//             BatchReader::ParquetReader { source_dir } => {
//                 let file =
//                     File::open(source_dir.join("contracts_config.parquet")).unwrap();
//                 Box::new(ParquetBatchReader::<File, ContractConfig>::new(file).unwrap())
//             }
//         }
//     }
//
//     pub fn contract_state_batches(self) -> Box<dyn BatchReaderTrait<ContractState>> {
//         match self {
//             BatchReader::JSONReader { source, batch_size } => Box::new(
//                 JsonBatchReader::<ContractState>::from_state(source, batch_size),
//             ),
//             BatchReader::ParquetReader { source_dir } => {
//                 let file = File::open(source_dir.join("contracts_state_config.parquet"))
//                     .unwrap();
//                 Box::new(ParquetBatchReader::<File, ContractState>::new(file).unwrap())
//             }
//         }
//     }
//
//     pub fn contracts_balance_batches(self) -> Box<dyn BatchReaderTrait<ContractBalance>> {
//         match self {
//             BatchReader::JSONReader { source, batch_size } => Box::new(
//                 JsonBatchReader::<ContractBalance>::from_state(source, batch_size),
//             ),
//             BatchReader::ParquetReader { source_dir } => {
//                 let file =
//                     File::open(source_dir.join("contracts_balance_config.parquet"))
//                         .unwrap();
//                 Box::new(ParquetBatchReader::<File, ContractBalance>::new(file).unwrap())
//             }
//         }
//     }
// }
