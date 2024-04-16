use std::fmt::Debug;

use fuel_core_storage::{
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use itertools::Itertools;

use crate::{
    config::table_entry::TableEntry,
    AsTable,
    ChainConfig,
    StateConfig,
    MAX_GROUP_SIZE,
};

pub struct Groups<T: Mappable> {
    iter: GroupIter<T>,
}

impl<T> Groups<T>
where
    T: Mappable,
{
    pub fn len(&self) -> usize {
        match &self.iter {
            GroupIter::InMemory { groups } => groups.len(),
            #[cfg(feature = "parquet")]
            GroupIter::Parquet { decoder } => decoder.num_groups(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> IntoIterator for Groups<T>
where
    T: Mappable,
    GroupIter<T>: Iterator,
{
    type IntoIter = GroupIter<T>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.iter
    }
}

pub enum GroupIter<T>
where
    T: Mappable,
{
    InMemory {
        groups: std::vec::IntoIter<anyhow::Result<Vec<TableEntry<T>>>>,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        decoder: super::parquet::decode::Decoder<std::fs::File>,
    },
}

#[cfg(feature = "parquet")]
impl<T> Iterator for GroupIter<T>
where
    T: Mappable,
    TableEntry<T>: serde::de::DeserializeOwned,
{
    type Item = anyhow::Result<Vec<TableEntry<T>>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            GroupIter::InMemory { groups } => groups.next(),
            GroupIter::Parquet { decoder } => {
                let group = decoder.next()?.and_then(|byte_group| {
                    byte_group
                        .into_iter()
                        .map(|group| {
                            postcard::from_bytes(&group).map_err(|e| anyhow::anyhow!(e))
                        })
                        .collect()
                });
                Some(group)
            }
        }
    }
}

#[cfg(not(feature = "parquet"))]
impl<T> Iterator for GroupIter<T>
where
    T: Mappable,
{
    type Item = anyhow::Result<Vec<TableEntry<T>>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            GroupIter::InMemory { groups } => groups.next(),
        }
    }
}

#[derive(Clone, Debug)]
enum DataSource {
    #[cfg(feature = "parquet")]
    Parquet {
        tables: std::collections::HashMap<String, std::path::PathBuf>,
        block_height: BlockHeight,
        da_block_height: DaBlockHeight,
    },
    InMemory {
        state: StateConfig,
        group_size: usize,
    },
}

#[derive(Clone, Debug)]
pub struct SnapshotReader {
    chain_config: ChainConfig,
    data_source: DataSource,
}

impl SnapshotReader {
    pub fn new_in_memory(chain_config: ChainConfig, state: StateConfig) -> Self {
        Self {
            chain_config,
            data_source: DataSource::InMemory {
                state,
                group_size: MAX_GROUP_SIZE,
            },
        }
    }

    #[cfg(feature = "test-helpers")]
    pub fn local_testnet() -> Self {
        let state = StateConfig::local_testnet();
        let chain_config = ChainConfig::local_testnet();
        Self {
            data_source: DataSource::InMemory {
                state,
                group_size: MAX_GROUP_SIZE,
            },
            chain_config,
        }
    }

    pub fn with_chain_config(self, chain_config: ChainConfig) -> Self {
        Self {
            chain_config,
            ..self
        }
    }

    pub fn with_state_config(self, state_config: StateConfig) -> Self {
        Self {
            data_source: DataSource::InMemory {
                state: state_config,
                group_size: MAX_GROUP_SIZE,
            },
            ..self
        }
    }

    #[cfg(feature = "std")]
    fn json(
        state_file: impl AsRef<std::path::Path>,
        chain_config: ChainConfig,
        group_size: usize,
    ) -> anyhow::Result<Self> {
        use anyhow::Context;
        let state = {
            let path = state_file.as_ref();
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("Could not open snapshot file: {path:?}"))?;
            serde_json::from_reader(&mut file)?
        };

        Ok(Self {
            data_source: DataSource::InMemory { state, group_size },
            chain_config,
        })
    }

    #[cfg(feature = "parquet")]
    fn parquet(
        tables: std::collections::HashMap<String, std::path::PathBuf>,
        block_height: std::path::PathBuf,
        da_block_height: std::path::PathBuf,
        chain_config: ChainConfig,
    ) -> anyhow::Result<Self> {
        let block_height = Self::read_block_height(&block_height)?;
        let da_block_height = Self::read_block_height(&da_block_height)?;
        Ok(Self {
            data_source: DataSource::Parquet {
                tables,
                block_height,
                da_block_height,
            },
            chain_config,
        })
    }

    #[cfg(feature = "parquet")]
    fn read_block_height<Height>(path: &std::path::Path) -> anyhow::Result<Height>
    where
        Height: serde::de::DeserializeOwned,
    {
        use super::parquet::decode::Decoder;

        let file = std::fs::File::open(path)?;
        let group = Decoder::new(file)?
            .next()
            .ok_or_else(|| anyhow::anyhow!("No block height found"))??;
        let block_height = group
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No block height found"))?;
        postcard::from_bytes(&block_height).map_err(Into::into)
    }

    #[cfg(feature = "std")]
    pub fn open(
        snapshot_metadata: crate::config::SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        Self::open_w_config(snapshot_metadata, MAX_GROUP_SIZE)
    }

    #[cfg(feature = "std")]
    pub fn open_w_config(
        snapshot_metadata: crate::config::SnapshotMetadata,
        json_group_size: usize,
    ) -> anyhow::Result<Self> {
        use crate::TableEncoding;
        use anyhow::Context;
        let chain_config = {
            let path = &snapshot_metadata.chain_config;
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("Could not open chain config file: {path:?}"))?;
            serde_json::from_reader(&mut file)?
        };

        match snapshot_metadata.table_encoding {
            TableEncoding::Json { filepath } => {
                Self::json(filepath, chain_config, json_group_size)
            }
            #[cfg(feature = "parquet")]
            TableEncoding::Parquet {
                tables,
                block_height,
                da_block_height,
                ..
            } => Self::parquet(tables, block_height, da_block_height, chain_config),
        }
    }

    pub fn read<T>(&self) -> anyhow::Result<Groups<T>>
    where
        T: TableWithBlueprint,
        StateConfig: AsTable<T>,
        TableEntry<T>: serde::de::DeserializeOwned,
    {
        let iter = match &self.data_source {
            #[cfg(feature = "parquet")]
            DataSource::Parquet { tables, .. } => {
                use anyhow::Context;
                use fuel_core_storage::kv_store::StorageColumn;
                let name = T::column().name();
                let Some(path) = tables.get(name) else {
                    return Ok(Groups {
                        iter: GroupIter::InMemory {
                            groups: vec![].into_iter(),
                        },
                    });
                };
                let file = std::fs::File::open(path).with_context(|| {
                    format!("Could not open {path:?} in order to read table '{name}'")
                })?;

                GroupIter::Parquet {
                    decoder: super::parquet::decode::Decoder::new(file)?,
                }
            }
            DataSource::InMemory { state, group_size } => {
                let collection = state
                    .as_table()
                    .into_iter()
                    .chunks(*group_size)
                    .into_iter()
                    .map(|vec_chunk| Ok(vec_chunk.collect()))
                    .collect_vec();
                GroupIter::InMemory {
                    groups: collection.into_iter(),
                }
            }
        };

        Ok(Groups { iter })
    }

    pub fn chain_config(&self) -> &ChainConfig {
        &self.chain_config
    }

    pub fn block_height(&self) -> BlockHeight {
        match &self.data_source {
            DataSource::InMemory { state, .. } => state.block_height,
            #[cfg(feature = "parquet")]
            DataSource::Parquet { block_height, .. } => *block_height,
        }
    }

    pub fn da_block_height(&self) -> DaBlockHeight {
        match &self.data_source {
            DataSource::InMemory { state, .. } => state.da_block_height,
            #[cfg(feature = "parquet")]
            DataSource::Parquet {
                da_block_height, ..
            } => *da_block_height,
        }
    }
}
