use std::{
    fmt::Debug,
    fs::File,
    io::Read,
    path::{
        Path,
        PathBuf,
    },
};

use super::parquet;
use itertools::Itertools;

use crate::{
    config::{
        contract_balance::ContractBalance,
        contract_state::ContractStateConfig,
    },
    CoinConfig,
    ContractConfig,
    Group,
    MessageConfig,
    StateConfig,
    WithId,
    WithIndex,
};

use super::WithIndexResult;

pub enum Iter<T> {
    InMemory {
        groups: std::vec::IntoIter<WithIndexResult<T>>,
    },
    Parquet {
        decoder: parquet::Decoder<File, T>,
    },
}

impl<T> Iterator for Iter<T>
where
    parquet::Decoder<File, T>: Iterator<Item = WithIndexResult<T>>,
{
    type Item = WithIndexResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Iter::InMemory { groups } => groups.next(),
            Iter::Parquet { decoder } => decoder.next(),
        }
    }
}

#[derive(Clone, Debug)]
enum DataSource {
    Parquet {
        snapshot_dir: PathBuf,
    },
    InMemory {
        state: StateConfig,
        group_size: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Decoder {
    data_source: DataSource,
}

impl Decoder {
    pub fn json(
        snapshot_dir: impl AsRef<Path>,
        group_size: usize,
    ) -> anyhow::Result<Self> {
        let path = snapshot_dir.as_ref().join("state.json");

        // This is a workaround until the Deserialize implementation is fixed to not require a
        // borrowed string over in fuel-vm.
        let mut contents = String::new();
        let mut file = std::fs::File::open(path)?;
        file.read_to_string(&mut contents)?;

        let state = serde_json::from_str(&contents)?;

        Ok(Self::in_memory(state, group_size))
    }

    pub fn in_memory(state: StateConfig, group_size: usize) -> Self {
        Self {
            data_source: DataSource::InMemory { state, group_size },
        }
    }

    pub fn parquet(snapshot_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_source: DataSource::Parquet {
                snapshot_dir: snapshot_dir.into(),
            },
        }
    }

    pub fn detect_encoding(
        snapshot_dir: impl AsRef<Path>,
        default_group_size: usize,
    ) -> anyhow::Result<Self> {
        let snapshot_dir = snapshot_dir.as_ref();

        if snapshot_dir.join("state.json").exists() {
            Ok(Self::json(snapshot_dir, default_group_size)?)
        } else {
            Ok(Self::parquet(snapshot_dir.to_owned()))
        }
    }

    pub fn coins(&self) -> anyhow::Result<Iter<Group<CoinConfig>>> {
        self.create_iterator(|state| &state.coins, "coins")
    }

    pub fn messages(&self) -> anyhow::Result<Iter<Group<MessageConfig>>> {
        self.create_iterator(|state| &state.messages, "messages")
    }

    pub fn contracts(&self) -> anyhow::Result<Iter<Group<ContractConfig>>> {
        self.create_iterator(|state| &state.contracts, "contracts")
    }

    pub fn contract_state(
        &self,
    ) -> anyhow::Result<Iter<WithId<Group<ContractStateConfig>>>> {
        todo!()
        // self.create_iterator(
        //     |state| {
        //         state
        //             .contract_state
        //             .iter()
        //             .map(|(id, state)| WithId { id, data: state })
        //             .collect()
        //     },
        //     "contract_state",
        // )
    }

    pub fn contract_balance(
        &self,
    ) -> anyhow::Result<Iter<WithId<Group<ContractBalance>>>> {
        todo!()
        // self.create_iterator(|state| &state.contract_balance, "contract_balance")
    }

    fn create_iterator<T: Clone>(
        &self,
        extractor: impl FnOnce(&StateConfig) -> &Vec<T>,
        parquet_filename: &'static str,
    ) -> anyhow::Result<Iter<Group<T>>> {
        match &self.data_source {
            DataSource::InMemory { state, group_size } => {
                let groups = extractor(state).clone();
                Ok(Self::in_memory_iter(groups, *group_size))
            }
            DataSource::Parquet { snapshot_dir } => {
                let path = snapshot_dir.join(format!("{parquet_filename}.parquet"));
                let file = std::fs::File::open(path)?;
                Ok(Iter::Parquet {
                    decoder: parquet::Decoder::new(file)?,
                })
            }
        }
    }

    fn in_memory_iter<T>(items: Vec<T>, group_size: usize) -> Iter<Group<T>> {
        let groups = items
            .into_iter()
            .chunks(group_size)
            .into_iter()
            .map(Itertools::collect_vec)
            .enumerate()
            .map(|(index, vec_chunk)| {
                Ok(WithIndex {
                    data: vec_chunk,
                    index,
                })
            })
            .collect_vec()
            .into_iter();

        Iter::InMemory { groups }
    }
}
