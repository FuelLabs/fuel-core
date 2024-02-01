use std::fmt::Debug;

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
};

use super::GroupResult;

pub enum IntoIter<T> {
    InMemory {
        groups: std::vec::IntoIter<GroupResult<T>>,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        decoder: super::parquet::PostcardDecoder<T>,
    },
}

#[cfg(feature = "parquet")]
impl<T> Iterator for IntoIter<T>
where
    super::parquet::PostcardDecoder<T>: Iterator<Item = GroupResult<T>>,
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
    Parquet { files: crate::ParquetFiles },
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
    #[cfg(feature = "std")]
    pub fn json(
        path: impl AsRef<std::path::Path>,
        group_size: usize,
    ) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path)?;

        let state = serde_json::from_reader(&mut file)?;

        Ok(Self::in_memory(state, group_size))
    }

    pub fn in_memory(state: StateConfig, group_size: usize) -> Self {
        Self {
            data_source: DataSource::InMemory { state, group_size },
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(files: crate::ParquetFiles) -> Self {
        Self {
            data_source: DataSource::Parquet { files },
        }
    }

    #[cfg(feature = "std")]
    pub fn for_snapshot(
        snapshot_metadata: crate::config::SnapshotMetadata,
        default_group_size: usize,
    ) -> anyhow::Result<Self> {
        use crate::StateEncoding;

        let decoder = match snapshot_metadata.take_state_encoding() {
            StateEncoding::Json { filepath } => Self::json(filepath, default_group_size)?,
            #[cfg(feature = "parquet")]
            StateEncoding::Parquet { filepaths, .. } => Self::parquet(filepaths),
        };
        Ok(decoder)
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
            DataSource::Parquet { files } => {
                let path = file_picker(files);
                let file = std::fs::File::open(path)?;
                Ok(IntoIter::Parquet {
                    decoder: super::parquet::Decoder::new(file)?,
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
