mod in_memory;
mod json;
mod parquet;

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
pub type DynStateDecoder = Box<dyn StateDecoderTrait>;

#[derive(Debug, Clone)]
pub enum StateSource {
    InMemory(StateConfig),
    Snapshot(PathBuf),
}

pub trait StateDecoderTrait {
    fn coins(&self) -> anyhow::Result<DynGroupDecoder<CoinConfig>>;
    fn messages(&self) -> anyhow::Result<DynGroupDecoder<MessageConfig>>;
    fn contracts(&self) -> anyhow::Result<DynGroupDecoder<ContractConfig>>;
    fn contract_state(&self) -> anyhow::Result<DynGroupDecoder<ContractState>>;
    fn contract_balance(&self) -> anyhow::Result<DynGroupDecoder<ContractBalance>>;
}

struct JsonStateDecoder {
    path: PathBuf,
    batch_size: usize,
}

impl JsonStateDecoder {
    fn new(snapshot_path: PathBuf, default_batch_size: usize) -> Self {
        Self {
            path: snapshot_path.join("state.json"),
            batch_size: default_batch_size,
        }
    }
}

impl JsonStateDecoder {
    fn decoder<T>(&self) -> anyhow::Result<DynGroupDecoder<T>>
    where
        T: 'static,
        JsonDecoder<T>: GroupDecoder<T>,
        in_memory::Decoder<StateConfig, T>: GroupDecoder<T>,
    {
        let file = std::fs::File::open(&self.path)?;
        let reader = JsonDecoder::<T>::new(file, self.batch_size)?;
        Ok(Box::new(reader))
    }
}

impl StateDecoderTrait for JsonStateDecoder {
    fn coins(&self) -> anyhow::Result<DynGroupDecoder<CoinConfig>> {
        self.decoder()
    }

    fn messages(&self) -> anyhow::Result<DynGroupDecoder<MessageConfig>> {
        self.decoder()
    }

    fn contracts(&self) -> anyhow::Result<DynGroupDecoder<ContractConfig>> {
        self.decoder()
    }

    fn contract_state(&self) -> anyhow::Result<DynGroupDecoder<ContractState>> {
        self.decoder()
    }

    fn contract_balance(&self) -> anyhow::Result<DynGroupDecoder<ContractBalance>> {
        self.decoder()
    }
}

#[derive(Debug, Clone)]
pub struct ParquetStateDecoder {
    snapshot_dir: PathBuf,
}

impl StateDecoderTrait for ParquetStateDecoder {
    fn coins(&self) -> anyhow::Result<DynGroupDecoder<CoinConfig>> {
        self.decoder("coins")
    }

    fn messages(&self) -> anyhow::Result<DynGroupDecoder<MessageConfig>> {
        self.decoder("messages")
    }

    fn contracts(&self) -> anyhow::Result<DynGroupDecoder<ContractConfig>> {
        self.decoder("contracts")
    }

    fn contract_state(&self) -> anyhow::Result<DynGroupDecoder<ContractState>> {
        self.decoder("contract_state")
    }

    fn contract_balance(&self) -> anyhow::Result<DynGroupDecoder<ContractBalance>> {
        self.decoder("contract_balance")
    }
}

impl ParquetStateDecoder {
    pub fn new(snapshot_dir: PathBuf) -> Self {
        Self { snapshot_dir }
    }

    fn decoder<T>(&self, parquet_filename: &str) -> anyhow::Result<DynGroupDecoder<T>>
    where
        T: 'static,
        ParquetBatchReader<std::fs::File, T>: GroupDecoder<T>,
    {
        let path = self
            .snapshot_dir
            .join(format!("{parquet_filename}.parquet"));
        let file = std::fs::File::open(path)?;
        Ok(Box::new(ParquetBatchReader::new(file)?))
    }
}

struct InMemoryStateDecoder {
    state: StateConfig,
    batch_size: usize,
}

impl InMemoryStateDecoder {
    fn new(state: StateConfig, batch_size: usize) -> Self {
        Self { state, batch_size }
    }

    fn decoder<T>(&self) -> anyhow::Result<DynGroupDecoder<T>>
    where
        T: 'static,
        in_memory::Decoder<StateConfig, T>: GroupDecoder<T>,
    {
        let reader = Decoder::new(self.state.clone(), self.batch_size);
        Ok(Box::new(reader))
    }
}

impl StateDecoderTrait for InMemoryStateDecoder {
    fn coins(&self) -> anyhow::Result<DynGroupDecoder<CoinConfig>> {
        self.decoder()
    }

    fn messages(&self) -> anyhow::Result<DynGroupDecoder<MessageConfig>> {
        self.decoder()
    }

    fn contracts(&self) -> anyhow::Result<DynGroupDecoder<ContractConfig>> {
        self.decoder()
    }

    fn contract_state(&self) -> anyhow::Result<DynGroupDecoder<ContractState>> {
        self.decoder()
    }

    fn contract_balance(&self) -> anyhow::Result<DynGroupDecoder<ContractBalance>> {
        self.decoder()
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
        ops::Deref,
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
            let batch_size = 100;
            let num_batches = 10;
            macro_rules! write_batches {
                ($data_type: ty, $write_method:ident) => {{
                    let batches = repeat_with(|| <$data_type>::random(&mut rng))
                        .chunks(batch_size)
                        .into_iter()
                        .map(|chunk| chunk.collect_vec())
                        .enumerate()
                        .map(|(index, data)| Group { index, data })
                        .take(num_batches)
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
            let state_reader: DynStateDecoder = match format {
                Format::Json => Box::new(JsonStateDecoder::new(path, batch_size)),
                Format::Parquet => Box::new(ParquetStateDecoder::new(path)),
                Format::InMemory => {
                    Box::new(InMemoryStateDecoder::new(state_config.clone(), batch_size))
                }
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
