use std::fmt::Debug;

use fuel_core_types::fuel_types::BlockHeight;
use itertools::Itertools;

use crate::{
    config::{
        contract_balance::ContractBalanceConfig,
        contract_state::ContractStateConfig,
    },
    CoinConfig,
    ContractConfig,
    Group,
    MessageConfig,
    StateConfig,
    MAX_GROUP_SIZE,
};

use super::GroupResult;

pub enum IntoIter<T> {
    InMemory {
        groups: std::vec::IntoIter<GroupResult<T>>,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        decoder: super::parquet::decode::PostcardDecoder<T>,
    },
}

#[cfg(feature = "parquet")]
impl<T> Iterator for IntoIter<T>
where
    super::parquet::decode::PostcardDecoder<T>: Iterator<Item = GroupResult<T>>,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::InMemory { groups } => groups.next(),
            IntoIter::Parquet { decoder } => decoder.next(),
        }
    }
}

#[cfg(not(feature = "parquet"))]
impl<T> Iterator for IntoIter<T> {
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::InMemory { groups } => groups.next(),
        }
    }
}

#[derive(Clone, Debug)]
enum DataSource {
    #[cfg(feature = "parquet")]
    Parquet {
        files: crate::ParquetFiles,
        block_height: BlockHeight,
    },
    InMemory {
        state: StateConfig,
        group_size: usize,
    },
}

#[derive(Clone, Debug)]
pub struct StateReader {
    data_source: DataSource,
}

impl StateReader {
    pub fn state_config(self) -> Option<StateConfig> {
        match self.data_source {
            DataSource::InMemory { state, .. } => Some(state),
            #[cfg(feature = "parquet")]
            DataSource::Parquet { .. } => None,
        }
    }

    #[cfg(feature = "std")]
    pub fn json(
        path: impl AsRef<std::path::Path>,
        group_size: usize,
    ) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path)?;

        let state = serde_json::from_reader(&mut file)?;

        Ok(Self {
            data_source: DataSource::InMemory { state, group_size },
        })
    }

    pub fn in_memory(state: StateConfig) -> Self {
        Self {
            data_source: DataSource::InMemory {
                state,
                group_size: MAX_GROUP_SIZE,
            },
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(files: crate::ParquetFiles) -> anyhow::Result<Self> {
        let block_height = Self::read_block_height(&files.block_height)?;
        Ok(Self {
            data_source: DataSource::Parquet {
                files,
                block_height,
            },
        })
    }

    #[cfg(feature = "parquet")]
    fn read_block_height(path: &std::path::Path) -> anyhow::Result<BlockHeight> {
        let file = std::fs::File::open(path)?;
        let group = super::parquet::decode::PostcardDecoder::new(file)?
            .next()
            .ok_or_else(|| anyhow::anyhow!("No block height found"))??
            .data;
        let block_height = group
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No block height found"))?;
        Ok(block_height)
    }

    #[cfg(feature = "std")]
    pub fn for_snapshot(
        snapshot_metadata: crate::config::SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        use crate::StateEncoding;

        match snapshot_metadata.take_state_encoding() {
            StateEncoding::Json { filepath } => Self::json(filepath, MAX_GROUP_SIZE),
            #[cfg(feature = "parquet")]
            StateEncoding::Parquet { filepaths, .. } => Self::parquet(filepaths),
        }
    }

    pub fn coins(&self) -> anyhow::Result<IntoIter<CoinConfig>> {
        self.create_iterator(
            |state| &state.coins,
            #[cfg(feature = "parquet")]
            |files| &files.coins,
        )
    }

    pub fn messages(&self) -> anyhow::Result<IntoIter<MessageConfig>> {
        self.create_iterator(
            |state| &state.messages,
            #[cfg(feature = "parquet")]
            |files| &files.messages,
        )
    }

    pub fn contracts(&self) -> anyhow::Result<IntoIter<ContractConfig>> {
        self.create_iterator(
            |state| &state.contracts,
            #[cfg(feature = "parquet")]
            |files| &files.contracts,
        )
    }

    pub fn contract_state(&self) -> anyhow::Result<IntoIter<ContractStateConfig>> {
        self.create_iterator(
            |state| &state.contract_state,
            #[cfg(feature = "parquet")]
            |files| &files.contract_state,
        )
    }

    pub fn contract_balance(&self) -> anyhow::Result<IntoIter<ContractBalanceConfig>> {
        self.create_iterator(
            |state| &state.contract_balance,
            #[cfg(feature = "parquet")]
            |files| &files.contract_balance,
        )
    }

    pub fn block_height(&self) -> BlockHeight {
        match &self.data_source {
            DataSource::InMemory { state, .. } => state.block_height,
            #[cfg(feature = "parquet")]
            DataSource::Parquet { block_height, .. } => *block_height,
        }
    }

    fn create_iterator<T: Clone>(
        &self,
        extractor: impl FnOnce(&StateConfig) -> &Vec<T>,
        #[cfg(feature = "parquet")] file_picker: impl FnOnce(
            &crate::ParquetFiles,
        ) -> &std::path::Path,
    ) -> anyhow::Result<IntoIter<T>> {
        match &self.data_source {
            DataSource::InMemory { state, group_size } => {
                let groups = extractor(state).clone();
                Ok(Self::in_memory_iter(groups, *group_size))
            }
            #[cfg(feature = "parquet")]
            DataSource::Parquet { files, .. } => {
                let path = file_picker(files);
                let file = std::fs::File::open(path)?;
                Ok(IntoIter::Parquet {
                    decoder: super::parquet::decode::Decoder::new(file)?,
                })
            }
        }
    }

    fn in_memory_iter<T>(items: Vec<T>, group_size: usize) -> IntoIter<T> {
        let groups = items
            .into_iter()
            .chunks(group_size)
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
            .into_iter();

        IntoIter::InMemory { groups }
    }
}
