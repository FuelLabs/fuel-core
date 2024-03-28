use std::fmt::Debug;

use fuel_core_storage::structured_storage::TableWithBlueprint;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use itertools::Itertools;

use crate::{
    config::table_entry::TableEntry,
    AsTable,
    ChainConfig,
    Group,
    GroupResult,
    StateConfig,
    MAX_GROUP_SIZE,
};

pub enum IntoIter<T> {
    InMemory {
        groups: std::vec::IntoIter<GroupResult<T>>,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        decoder: super::parquet::decode::Decoder<std::fs::File>,
    },
}

#[cfg(feature = "parquet")]
impl<T> Iterator for IntoIter<T>
where
    T: serde::de::DeserializeOwned,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::InMemory { groups } => groups.next(),
            IntoIter::Parquet { decoder } => {
                let group = decoder.next()?.and_then(|bytes_group| {
                    let decoded = bytes_group
                        .data
                        .into_iter()
                        .map(|bytes| postcard::from_bytes(&bytes))
                        .try_collect()?;
                    Ok(Group {
                        index: bytes_group.index,
                        data: decoded,
                    })
                });
                Some(group)
            }
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
            .ok_or_else(|| anyhow::anyhow!("No block height found"))??
            .data;
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

    pub fn read<T>(&self) -> anyhow::Result<IntoIter<TableEntry<T>>>
    where
        T: TableWithBlueprint,
        StateConfig: AsTable<T>,
        TableEntry<T>: serde::de::DeserializeOwned,
    {
        match &self.data_source {
            #[cfg(feature = "parquet")]
            DataSource::Parquet { tables, .. } => {
                use anyhow::Context;
                use fuel_core_storage::kv_store::StorageColumn;
                let name = T::column().name();
                let path = tables.get(name).ok_or_else(|| {
                    anyhow::anyhow!("table '{name}' not found in snapshot metadata.")
                })?;
                let file = std::fs::File::open(path).with_context(|| {
                    format!("Could not open {path:?} in order to read table '{name}'")
                })?;

                Ok(IntoIter::Parquet {
                    decoder: super::parquet::decode::Decoder::new(file)?,
                })
            }
            DataSource::InMemory { state, group_size } => {
                let collection = state
                    .as_table()
                    .into_iter()
                    .chunks(*group_size)
                    .into_iter()
                    .enumerate()
                    .map(|(index, vec_chunk)| {
                        Ok(Group {
                            data: vec_chunk.collect(),
                            index,
                        })
                    })
                    .collect_vec();
                Ok(IntoIter::InMemory {
                    groups: collection.into_iter(),
                })
            }
        }
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
