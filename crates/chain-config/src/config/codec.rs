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

#[derive(Debug, Clone)]
pub enum StateSource {
    InMemory(StateConfig),
    Snapshot(PathBuf),
}

#[derive(Debug, Clone)]
pub struct StateDecoder {
    default_batch_size: usize,
    source: StateSource,
}

impl StateDecoder {
    pub fn new(source: StateSource, json_batch_size: usize) -> Self {
        Self {
            source,
            default_batch_size: json_batch_size,
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
        ParquetBatchReader<std::fs::File, T>: GroupDecoder<T>,
        JsonDecoder<T>: GroupDecoder<T>,
        in_memory::Decoder<T>: GroupDecoder<T>,
    {
        let reader = match &self.source {
            StateSource::Snapshot(path) => {
                let json_path = path.join("state.json");
                if json_path.exists() {
                    let file = std::fs::File::open(json_path)?;
                    let reader = JsonDecoder::<T>::new(file, self.default_batch_size)?;
                    Box::new(reader) as DynGroupDecoder<T>
                } else {
                    let path = path.join(format!("{parquet_filename}.parquet"));
                    let file = std::fs::File::open(path)?;
                    Box::new(ParquetBatchReader::new(file)?)
                }
            }
            StateSource::InMemory(state_config) => {
                let decoder = in_memory::Decoder::<T>::new(
                    state_config.clone(),
                    self.default_batch_size,
                );
                Box::new(decoder)
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
    use std::iter::repeat_with;

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

    #[test]
    fn can_read_write() {
        enum Format {
            Json,
            Parquet,
        }
        let test = |format: Format| {
            let temp_dir = tempfile::tempdir().unwrap();

            let path = temp_dir.path();
            let mut writer = match format {
                Format::Parquet => parquet_writer(path).unwrap(),
                Format::Json => json_writer(path).unwrap(),
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

            let state_reader = StateDecoder::new(
                StateSource::Snapshot(temp_dir.path().to_path_buf()),
                batch_size,
            );
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
    }
}
