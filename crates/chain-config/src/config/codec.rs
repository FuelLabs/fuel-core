mod in_memory;
mod json;
mod parquet;

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use ::parquet::basic::{Compression, GzipLevel};
pub use in_memory::*;
pub use json::*;
pub use parquet::*;

use crate::{CoinConfig, ContractConfig, MessageConfig, StateConfig};

use super::{contract_balance::ContractBalance, contract_state::ContractState};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group<T> {
    pub index: usize,
    pub data: Vec<T>,
}

type GroupResult<T> = anyhow::Result<Group<T>>;
pub trait GroupDecoder<T>: Iterator<Item = GroupResult<T>> {}

pub trait GroupEncoder {
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

pub type DynGroupDecoder<T> = Box<dyn GroupDecoder<T>>;

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

    pub fn coins(&self) -> anyhow::Result<DynGroupDecoder<CoinConfig>> {
        self.decoder("coins")
    }

    pub fn messages(&self) -> anyhow::Result<DynGroupDecoder<MessageConfig>> {
        self.decoder("messages")
    }

    pub fn contracts(&self) -> anyhow::Result<DynGroupDecoder<ContractConfig>> {
        self.decoder("contracts")
    }

    pub fn contract_state(&self) -> anyhow::Result<DynGroupDecoder<ContractState>> {
        self.decoder("contract_state")
    }

    pub fn contract_balance(&self) -> anyhow::Result<DynGroupDecoder<ContractBalance>> {
        self.decoder("contract_balance")
    }

    fn decoder<T>(&self, parquet_filename: &str) -> anyhow::Result<DynGroupDecoder<T>>
    where
        T: 'static,
        JsonDecoder<T>: GroupDecoder<T>,
        ParquetBatchReader<std::fs::File, T>: GroupDecoder<T>,
        in_memory::Decoder<StateConfig, T>: GroupDecoder<T>,
    {
        let reader = match &self {
            StateDecoder::Json { path, group_size } => {
                let file = std::fs::File::open(path.join("state.json"))?;
                let reader = JsonDecoder::<T>::new(file, *group_size)?;
                Box::new(reader)
            }
            StateDecoder::Parquet { path } => {
                let path = path.join(format!("{parquet_filename}.parquet"));
                let file = std::fs::File::open(path)?;
                Box::new(ParquetBatchReader::new(file)?) as DynGroupDecoder<T>
            }
            StateDecoder::InMemory { state, group_size } => {
                let reader = Decoder::new(state.clone(), *group_size);
                Box::new(reader)
            }
        };

        Ok(reader)
    }
}

fn parquet_writer(
    snapshot_dir: impl AsRef<Path>,
) -> anyhow::Result<Box<dyn GroupEncoder>> {
    let snapshot_location = snapshot_dir.as_ref();
    let open_file = |name| {
        std::fs::File::create(snapshot_location.join(format!("{name}.parquet"))).unwrap()
    };

    let writer = ParquetEncoder::new(
        open_file("coins"),
        open_file("messages"),
        open_file("contracts"),
        open_file("contract_state"),
        open_file("contract_balance"),
        Compression::GZIP(GzipLevel::try_new(1)?),
    )?;

    Ok(Box::new(writer))
}

fn json_writer(snapshot_dir: impl AsRef<Path>) -> anyhow::Result<Box<dyn GroupEncoder>> {
    let state = std::fs::File::create(snapshot_dir.as_ref().join("state.json"))?;
    let writer = JsonBatchWriter::new(state);
    Ok(Box::new(writer))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{RefCell, RefMut},
        iter::repeat_with,
        rc::Rc,
    };

    use itertools::Itertools;

    use super::*;

    fn assert_batches_identical<T>(
        original: &[Group<T>],
        read: impl Iterator<Item = Result<Group<T>, anyhow::Error>>,
    ) where
        Vec<T>: PartialEq,
        T: PartialEq + std::fmt::Debug,
    {
        pretty_assertions::assert_eq!(
            original,
            read.collect::<Result<Vec<_>, _>>().unwrap()
        );
    }

    #[derive(Default, Clone)]
    struct PeekableStateConfig {
        inner: Rc<RefCell<StateConfig>>,
    }
    impl PeekableStateConfig {
        fn gimme(&mut self) -> RefMut<StateConfig> {
            self.inner.borrow_mut()
        }
    }

    // impl BorrowMut<StateConfig> for PeekableStateConfig {
    //     fn borrow_mut(&mut self) -> &mut StateConfig {
    //         RefCell::borrow_mut(Rc::borrow(&self.inner))
    //     }
    // }
    //
    // impl Borrow<StateConfig> for PeekableStateConfig {
    //     fn borrow(&self) -> &StateConfig {
    //         todo!()
    //     }
    // }

    #[test]
    fn can_read_write() {
        enum Format {
            Json,
            Parquet,
            InMemory,
        }
        let test = |format: Format| {
            let temp_dir = tempfile::tempdir().unwrap();

            let path = temp_dir.path();
            let mut state_config = StateConfig::default();
            let mut writer = match format {
                Format::Parquet => parquet_writer(path).unwrap(),
                Format::Json => json_writer(path).unwrap(),
                Format::InMemory => Box::new(Encoder::new(&mut state_config)),
            };

            let mut rng = rand::thread_rng();
            let group_size = 100;
            let num_groups = 10;
            macro_rules! write_batches {
                ($data_type: ty, $write_method:ident) => {{
                    let batches = repeat_with(|| <$data_type>::random(&mut rng))
                        .chunks(group_size)
                        .into_iter()
                        .map(|chunk| chunk.collect_vec())
                        .enumerate()
                        .map(|(index, data)| Group { index, data })
                        .take(num_groups)
                        .collect_vec();

                    for batch in &batches {
                        writer.$write_method(batch.data.clone()).unwrap();
                    }
                    batches
                }};
            }

            let coin_batches = write_batches!(CoinConfig, write_coins);
            let message_batches = write_batches!(MessageConfig, write_messages);
            let contract_batches = write_batches!(ContractConfig, write_contracts);
            let contract_state_batches =
                write_batches!(ContractState, write_contract_state);
            let contract_balance_batches =
                write_batches!(ContractBalance, write_contract_balance);
            writer.close().unwrap();

            let path = temp_dir.path().into();
            let state_reader = match format {
                Format::Json => StateDecoder::Json { path, group_size },
                Format::Parquet => StateDecoder::Parquet { path },
                Format::InMemory => StateDecoder::InMemory {
                    state: state_config.clone(),
                    group_size,
                },
            };
            assert_batches_identical(&coin_batches, state_reader.coins().unwrap());
            assert_batches_identical(&message_batches, state_reader.messages().unwrap());
            assert_batches_identical(
                &contract_batches,
                state_reader.contracts().unwrap(),
            );
            assert_batches_identical(
                &contract_state_batches,
                state_reader.contract_state().unwrap(),
            );
            assert_batches_identical(
                &contract_balance_batches,
                state_reader.contract_balance().unwrap(),
            );
        };

        test(Format::Json);
        test(Format::Parquet);
        test(Format::InMemory);
    }
}
