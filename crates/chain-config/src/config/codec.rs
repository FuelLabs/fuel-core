pub mod in_memory;
mod json;
mod parquet;

use std::{
    fmt::Debug,
    fs::File,
    io::Read,
    path::{
        Path,
        PathBuf,
    },
};

use ::parquet::basic::{
    Compression,
    GzipLevel,
};
use itertools::Itertools;

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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group<T> {
    pub index: usize,
    pub data: Vec<T>,
}

type GroupResult<T> = anyhow::Result<Group<T>>;

pub enum IntoIter<T> {
    InMemory {
        groups: std::vec::IntoIter<GroupResult<T>>,
    },
    Parquet {
        decoder: parquet::Decoder<File, T>,
    },
}

impl<T> Iterator for IntoIter<T>
where
    parquet::Decoder<File, T>: Iterator<Item = GroupResult<T>>,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::InMemory { groups } => groups.next(),
            IntoIter::Parquet { decoder } => decoder.next(),
        }
    }
}

pub trait GroupDecoder<T>: Iterator<Item = GroupResult<T>> {}
pub type DynStateEncoder = Box<dyn StateEncoder>;

pub trait StateEncoder {
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

#[derive(Clone, Debug)]
pub enum StateDecoder {
    Json {
        path: PathBuf,
        group_size: usize,
    },
    Parquet {
        path: PathBuf,
    },
    InMemory {
        state: StateConfig,
        group_size: usize,
    },
}

impl StateDecoder {
    pub fn detect_state_encoding(
        snapshot_dir: impl AsRef<Path>,
        default_group_size: usize,
    ) -> Self {
        let json_path = snapshot_dir.as_ref().join("state.json");
        if json_path.exists() {
            Self::Json {
                path: json_path,
                group_size: default_group_size,
            }
        } else {
            Self::Parquet {
                path: snapshot_dir.as_ref().into(),
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

    pub fn coins(&self) -> anyhow::Result<IntoIter<CoinConfig>> {
        self.create_iterator(|state| state.coins, "coins")
    }

    pub fn messages(&self) -> anyhow::Result<IntoIter<MessageConfig>> {
        self.create_iterator(|state| state.messages, "messages")
    }

    pub fn contracts(&self) -> anyhow::Result<IntoIter<ContractConfig>> {
        self.create_iterator(|state| state.contracts, "contracts")
    }

    pub fn contract_state(&self) -> anyhow::Result<IntoIter<ContractState>> {
        self.create_iterator(|state| state.contract_state, "contract_state")
    }

    pub fn contract_balance(&self) -> anyhow::Result<IntoIter<ContractBalance>> {
        self.create_iterator(|state| state.contract_balance, "contract_balance")
    }

    fn create_iterator<T: Clone>(
        &self,
        extractor: impl FnOnce(StateConfig) -> Vec<T>,
        parquet_filename: &'static str,
    ) -> anyhow::Result<IntoIter<T>> {
        match self {
            StateDecoder::Json { path, group_size } => {
                // This is a workaround until the Deserialize implementation is fixed to not require a
                // borrowed string over in fuel-vm.
                let path = path.join("state.json");

                let mut contents = String::new();
                let mut reader = std::fs::File::open(path)?;
                reader.read_to_string(&mut contents)?;

                let state = serde_json::from_str(&contents)?;
                let groups = extractor(state);
                Ok(Self::in_memory_iter(groups, *group_size))
            }
            StateDecoder::InMemory { state, group_size } => {
                let groups = extractor(state.clone());
                Ok(Self::in_memory_iter(groups, *group_size))
            }
            StateDecoder::Parquet { path } => {
                let path = path.join(format!("{parquet_filename}.parquet"));
                let file = std::fs::File::open(path)?;
                Ok(IntoIter::Parquet {
                    decoder: parquet::Decoder::new(file)?,
                })
            }
        }
    }
}

pub fn parquet_writer(snapshot_dir: impl AsRef<Path>) -> anyhow::Result<DynStateEncoder> {
    let snapshot_location = snapshot_dir.as_ref();
    let open_file = |name| {
        std::fs::File::create(snapshot_location.join(format!("{name}.parquet"))).unwrap()
    };

    let writer = parquet::Encoder::new(
        open_file("coins"),
        open_file("messages"),
        open_file("contracts"),
        open_file("contract_state"),
        open_file("contract_balance"),
        Compression::GZIP(GzipLevel::try_new(1)?),
    )?;

    Ok(Box::new(writer))
}

pub fn json_writer(snapshot_dir: impl AsRef<Path>) -> anyhow::Result<DynStateEncoder> {
    let state = std::fs::File::create(snapshot_dir.as_ref().join("state.json"))?;
    let writer = json::Encoder::new(state);
    Ok(Box::new(writer))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        ops::Range,
        rc::Rc,
    };

    use itertools::Itertools;

    use super::*;

    #[test]
    fn writes_then_reads_written() {
        let group_size = 100;
        let num_groups = 10;
        let starting_group_index = 3;
        {
            // In memory
            let state_config = Rc::new(RefCell::new(StateConfig::default()));
            let state_encoder =
                Box::new(in_memory::Encoder::new(Rc::clone(&state_config)));

            let init_decoder = || {
                let state_config: &StateConfig = &state_config.borrow();
                StateDecoder::InMemory {
                    state: state_config.clone(),
                    group_size,
                }
            };

            test_write_read(
                state_encoder,
                init_decoder,
                group_size,
                starting_group_index..num_groups,
            );
        }
        {
            // Json
            let temp_dir = tempfile::tempdir().unwrap();
            let state_encoder = json_writer(temp_dir.path()).unwrap();

            let init_decoder = || StateDecoder::Json {
                path: temp_dir.path().to_owned(),
                group_size,
            };

            test_write_read(
                state_encoder,
                init_decoder,
                group_size,
                starting_group_index..num_groups,
            )
        }
        {
            // Parquet
            let temp_dir = tempfile::tempdir().unwrap();
            let state_encoder = parquet_writer(temp_dir.path()).unwrap();

            let init_decoder = || StateDecoder::Parquet {
                path: temp_dir.path().to_owned(),
            };

            test_write_read(
                state_encoder,
                init_decoder,
                group_size,
                starting_group_index..num_groups,
            )
        }
    }

    fn test_write_read(
        mut encoder: DynStateEncoder,
        init_decoder: impl FnOnce() -> StateDecoder,
        group_size: usize,
        group_range: Range<usize>,
    ) {
        let num_groups = group_range.end;
        let mut rng = rand::thread_rng();
        macro_rules! write_batches {
            ($data_type: ty, $write_method:ident) => {{
                let batches = ::std::iter::repeat_with(|| <$data_type>::random(&mut rng))
                    .chunks(group_size)
                    .into_iter()
                    .map(|chunk| chunk.collect_vec())
                    .enumerate()
                    .map(|(index, data)| Group { index, data })
                    .take(num_groups)
                    .collect_vec();

                for batch in &batches {
                    encoder.$write_method(batch.data.clone()).unwrap();
                }
                batches
            }};
        }

        let coin_batches = write_batches!(CoinConfig, write_coins);
        let message_batches = write_batches!(MessageConfig, write_messages);
        let contract_batches = write_batches!(ContractConfig, write_contracts);
        let contract_state_batches = write_batches!(ContractState, write_contract_state);
        let contract_balance_batches =
            write_batches!(ContractBalance, write_contract_balance);
        encoder.close().unwrap();

        let state_reader = init_decoder();

        let skip_first = group_range.start;
        assert_batches_identical(
            &coin_batches,
            state_reader.coins().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &message_batches,
            state_reader.messages().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &contract_batches,
            state_reader.contracts().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &contract_state_batches,
            state_reader.contract_state().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &contract_balance_batches,
            state_reader.contract_balance().unwrap(),
            skip_first,
        );
    }

    fn assert_batches_identical<T>(
        original: &[Group<T>],
        read: impl Iterator<Item = Result<Group<T>, anyhow::Error>>,
        skip: usize,
    ) where
        Vec<T>: PartialEq,
        T: PartialEq + std::fmt::Debug,
    {
        pretty_assertions::assert_eq!(
            original[skip..],
            read.skip(skip).collect::<Result<Vec<_>, _>>().unwrap()
        );
    }
}
