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

// pub struct BatchReader<T, I: IntoIterator<Item = anyhow::Result<Batch<T>>>> {}
pub enum BatchReader {
    JSONReader {
        source: ChainState,
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

    pub fn message_batches(
        self,
    ) -> Box<
        dyn IntoIterator<
            Item = anyhow::Result<Batch<MessageConfig>>,
            IntoIter = Box<dyn Iterator<Item = anyhow::Result<Batch<MessageConfig>>>>,
        >,
    > {
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

    pub fn contract_batches(
        self,
    ) -> Box<
        dyn IntoIterator<
            Item = anyhow::Result<Batch<ContractConfig>>,
            IntoIter = Box<dyn Iterator<Item = anyhow::Result<Batch<ContractConfig>>>>,
        >,
    > {
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

    pub fn contract_state_batches(
        self,
    ) -> Box<
        dyn IntoIterator<
            Item = anyhow::Result<Batch<ContractState>>,
            IntoIter = Box<dyn Iterator<Item = anyhow::Result<Batch<ContractState>>>>,
        >,
    > {
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

    pub fn contracts_balance_batches(
        self,
    ) -> Box<
        dyn IntoIterator<
            Item = anyhow::Result<Batch<ContractBalance>>,
            IntoIter = Box<dyn Iterator<Item = anyhow::Result<Batch<ContractBalance>>>>,
        >,
    > {
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

pub trait BatchGenerator<T> {
    fn next_batch(&mut self) -> Option<anyhow::Result<Batch<T>>>;
}

pub trait BatchWriter<T> {
    fn write_batch(&mut self, elements: Vec<T>) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "random")]
    #[test]
    fn test_codec_batch() {
        let govno = BatchReader::JSONReader {
            source: ChainState::random(100, 100, &mut rand::thread_rng()),
            batch_size: 1,
        };

        for i in govno.coin_batches().into_iter() {
            for ele in i.unwrap().data {
                dbg!(ele);
            }
        }
    }
}
