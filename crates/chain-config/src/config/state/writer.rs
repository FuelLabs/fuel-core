use std::{
    fs::File,
    path::{
        Path,
        PathBuf,
    },
};

use ::parquet::basic::{
    Compression,
    GzipLevel,
};
use anyhow::Context;

use crate::{
    config::{
        contract_balance::ContractBalanceConfig,
        contract_state::ContractStateConfig,
        state::parquet::Schema,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

use super::parquet;

enum StateWriterType {
    Json {
        buffer: StateConfig,
        state_file_path: PathBuf,
    },
    Parquet {
        coins: parquet::Encoder<File, CoinConfig>,
        messages: parquet::Encoder<File, MessageConfig>,
        contracts: parquet::Encoder<File, ContractConfig>,
        contract_state: parquet::Encoder<File, ContractStateConfig>,
        contract_balance: parquet::Encoder<File, ContractBalanceConfig>,
    },
}

pub struct StateWriter {
    encoder: StateWriterType,
}

impl StateWriter {
    pub fn json(snapshot_dir: impl AsRef<Path>) -> Self {
        Self {
            encoder: StateWriterType::Json {
                state_file_path: snapshot_dir.as_ref().join("state_config.json"),
                buffer: StateConfig::default(),
            },
        }
    }

    pub fn parquet(
        snapshot_dir: impl AsRef<Path>,
        compression_level: u8,
    ) -> anyhow::Result<Self> {
        fn create_encoder<T>(
            path: &Path,
            name: &str,
            compression: Compression,
        ) -> anyhow::Result<parquet::Encoder<File, T>>
        where
            T: Schema,
        {
            let path = path.join(format!("{name}.parquet"));
            let file = std::fs::File::create(&path)
                .with_context(|| format!("Cannot open file ({path:?}) for writing"))?;
            parquet::Encoder::new(file, compression)
        }

        let path = snapshot_dir.as_ref();
        let compression =
            Compression::GZIP(GzipLevel::try_new(u32::from(compression_level))?);
        Ok(Self {
            encoder: StateWriterType::Parquet {
                coins: create_encoder(path, "coins", compression)?,
                messages: create_encoder(path, "messages", compression)?,
                contracts: create_encoder(path, "contracts", compression)?,
                contract_state: create_encoder(path, "contract_state", compression)?,
                contract_balance: create_encoder(path, "contract_balance", compression)?,
            },
        })
    }

    pub fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        match &mut self.encoder {
            StateWriterType::Json { buffer: state, .. } => {
                state.coins.extend(elements);
                Ok(())
            }
            StateWriterType::Parquet { coins, .. } => coins.write(elements),
        }
    }

    pub fn write_contracts(
        &mut self,
        elements: Vec<ContractConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            StateWriterType::Json { buffer: state, .. } => {
                state.contracts.extend(elements);
                Ok(())
            }
            StateWriterType::Parquet { contracts, .. } => contracts.write(elements),
        }
    }

    pub fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        match &mut self.encoder {
            StateWriterType::Json { buffer: state, .. } => {
                state.messages.extend(elements);
                Ok(())
            }
            StateWriterType::Parquet { messages, .. } => messages.write(elements),
        }
    }

    pub fn write_contract_state(
        &mut self,
        elements: Vec<ContractStateConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            StateWriterType::Json { buffer: state, .. } => {
                state.contract_state.extend(elements);
                Ok(())
            }
            StateWriterType::Parquet { contract_state, .. } => {
                contract_state.write(elements)
            }
        }
    }

    pub fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalanceConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            StateWriterType::Json { buffer: state, .. } => {
                state.contract_balance.extend(elements);
                Ok(())
            }
            StateWriterType::Parquet {
                contract_balance, ..
            } => contract_balance.write(elements),
        }
    }

    pub fn close(self) -> anyhow::Result<()> {
        match self.encoder {
            StateWriterType::Json {
                buffer,
                state_file_path,
            } => {
                let file = std::fs::File::create(state_file_path)?;
                serde_json::to_writer(file, &buffer)?;
                Ok(())
            }
            StateWriterType::Parquet {
                coins,
                messages,
                contracts,
                contract_state,
                contract_balance,
            } => {
                coins.close()?;
                messages.close()?;
                contracts.close()?;
                contract_state.close()?;
                contract_balance.close()?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fmt::Debug,
    };

    use crate::{
        Group,
        Randomize,
    };

    use super::*;
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    #[test]
    fn state_writer_json_generates_single_file_with_expected_name() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder = StateWriter::json(dir.path());

        // when
        encoder.close().unwrap();

        // then
        let entries: Vec<_> = dir.path().read_dir().unwrap().try_collect().unwrap();

        match entries.as_slice() {
            [entry] => assert_eq!(entry.path(), dir.path().join("state_config.json")),
            _ => panic!("Expected single file \"state_config.json\""),
        }
    }

    #[test]
    fn parquet_encoder_generates_expected_filenames() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder = StateWriter::parquet(dir.path(), 0).unwrap();

        // when
        encoder.close().unwrap();

        // then
        let entries: HashSet<_> = dir
            .path()
            .read_dir()
            .unwrap()
            .map_ok(|entry| entry.path())
            .try_collect()
            .unwrap();
        let expected_files = HashSet::from(
            [
                "coins.parquet",
                "messages.parquet",
                "contracts.parquet",
                "contract_state.parquet",
                "contract_balance.parquet",
            ]
            .map(|name| dir.path().join(name)),
        );

        assert_eq!(entries, expected_files);
    }

    #[test]
    fn parquet_encoder_encodes_types_in_expected_files() {
        let mut rng = StdRng::seed_from_u64(0);

        test_data_written_in_expected_file::<CoinConfig>(
            &mut rng,
            "coins.parquet",
            |coins, encoder| encoder.write_coins(coins),
        );

        test_data_written_in_expected_file::<MessageConfig>(
            &mut rng,
            "messages.parquet",
            |coins, encoder| encoder.write_messages(coins),
        );

        test_data_written_in_expected_file::<ContractConfig>(
            &mut rng,
            "contracts.parquet",
            |coins, encoder| encoder.write_contracts(coins),
        );

        test_data_written_in_expected_file::<ContractStateConfig>(
            &mut rng,
            "contract_state.parquet",
            |coins, encoder| encoder.write_contract_state(coins),
        );

        test_data_written_in_expected_file::<ContractBalanceConfig>(
            &mut rng,
            "contract_balance.parquet",
            |coins, encoder| encoder.write_contract_balance(coins),
        );
    }

    fn test_data_written_in_expected_file<T>(
        rng: impl rand::Rng,
        expected_filename: &str,
        write: impl FnOnce(Vec<T>, &mut StateWriter) -> anyhow::Result<()>,
    ) where
        parquet::Decoder<File, T>: Iterator<Item = anyhow::Result<Group<T>>>,
        T: Randomize + PartialEq + Debug + Clone,
    {
        // given
        let dir = tempfile::tempdir().unwrap();
        let mut encoder = StateWriter::parquet(dir.path(), 0).unwrap();
        let original_data = vec![T::randomize(rng)];

        // when
        write(original_data.clone(), &mut encoder).unwrap();
        encoder.close().unwrap();

        // then
        let file = std::fs::File::open(dir.path().join(expected_filename)).unwrap();
        let decoded = parquet::Decoder::<File, T>::new(file)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(original_data, decoded.first().unwrap().data);
    }
}
