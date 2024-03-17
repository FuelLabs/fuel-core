use std::{
    collections::HashMap,
    fmt::Debug,
};

use anyhow::Context;
use fuel_core_storage::{
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
    },
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use itertools::Itertools;

use crate::{
    config::{
        contract_balance::ContractBalanceConfig,
        contract_state::ContractStateConfig,
        my_entry::MyEntry,
    },
    AsTable,
    CoinConfig,
    ContractConfig,
    Group,
    GroupResult,
    MessageConfig,
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
            IntoIter::Parquet { decoder } => Some(decoder.next()?.map(|bytes_group| {
                let decoded = bytes_group
                    .data
                    .into_iter()
                    .map(|bytes| postcard::from_bytes(&bytes).unwrap())
                    .collect();
                Group {
                    index: bytes_group.index,
                    data: decoded,
                }
            })),
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
        tables: HashMap<String, std::path::PathBuf>,
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
    data_source: DataSource,
}

impl SnapshotReader {
    pub fn state_config(self) -> Option<StateConfig> {
        match self.data_source {
            DataSource::InMemory { state, .. } => Some(state),
            #[cfg(feature = "parquet")]
            DataSource::Parquet { .. } => None,
        }
    }

    #[cfg(feature = "std")]
    fn json(
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
    fn parquet(
        tables: HashMap<String, std::path::PathBuf>,
        block_height: std::path::PathBuf,
        da_block_height: std::path::PathBuf,
    ) -> anyhow::Result<Self> {
        let block_height = Self::read_block_height(&block_height)?;
        let da_block_height = Self::read_block_height(&da_block_height)?;
        Ok(Self {
            data_source: DataSource::Parquet {
                tables,
                block_height,
                da_block_height,
            },
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
    pub fn for_snapshot(
        snapshot_metadata: crate::config::SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        use crate::TableEncoding;

        match snapshot_metadata.table_encoding {
            TableEncoding::Json { filepath } => Self::json(filepath, MAX_GROUP_SIZE),
            #[cfg(feature = "parquet")]
            TableEncoding::Parquet {
                tables,
                block_height,
                da_block_height,
                ..
            } => Self::parquet(tables, da_block_height, block_height),
        }
    }

    pub fn rdr<T>(&self) -> anyhow::Result<IntoIter<MyEntry<T>>>
    where
        T: TableWithBlueprint,
        StateConfig: AsTable<T>,
        MyEntry<T>: serde::de::DeserializeOwned,
    {
        match &self.data_source {
            DataSource::Parquet { tables, .. } => {
                let name = T::column().name();
                let path = tables.get(name).ok_or_else(|| {
                    anyhow::anyhow!("table '{name}' not found snapshot metadata.")
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

    pub fn coins(&self) -> anyhow::Result<IntoIter<MyEntry<Coins>>> {
        self.rdr()
    }

    pub fn messages(&self) -> anyhow::Result<IntoIter<MyEntry<Messages>>> {
        self.rdr()
    }

    pub fn contracts_raw_code(
        &self,
    ) -> anyhow::Result<IntoIter<MyEntry<ContractsRawCode>>> {
        self.rdr()
    }

    pub fn contracts_info(&self) -> anyhow::Result<IntoIter<MyEntry<ContractsInfo>>> {
        self.rdr()
    }

    pub fn contracts_latest_utxo(
        &self,
    ) -> anyhow::Result<IntoIter<MyEntry<ContractsLatestUtxo>>> {
        self.rdr()
    }

    pub fn contract_state(&self) -> anyhow::Result<IntoIter<MyEntry<ContractsState>>> {
        self.rdr()
    }

    pub fn contract_balance(&self) -> anyhow::Result<IntoIter<MyEntry<ContractsAssets>>> {
        self.rdr()
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

    // fn create_iterator<T: Clone>(
    //     &self,
    //     extractor: impl FnOnce(&StateConfig) -> &Vec<T>,
    //     #[cfg(feature = "parquet")] file_picker: impl FnOnce(
    //         &crate::ParquetFiles,
    //     ) -> &std::path::Path,
    // ) -> anyhow::Result<IntoIter<T>> {
    //     match &self.data_source {
    //         DataSource::InMemory { state, group_size } => {
    //             let groups = extractor(state).clone();
    //             Ok(Self::in_memory_iter(groups, *group_size))
    //         }
    //         #[cfg(feature = "parquet")]
    //         DataSource::Parquet { files, .. } => {
    //             let path = file_picker(files);
    //             let file = std::fs::File::open(path)?;
    //             Ok(IntoIter::Parquet {
    //                 decoder: super::parquet::decode::Decoder::new(file)?,
    //             })
    //         }
    //     }
    // }

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
