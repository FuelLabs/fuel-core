use std::path::{
    Path,
    PathBuf,
};

use crate::{
    config::{
        contract_balance::ContractBalanceConfig,
        contract_state::ContractStateConfig,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

#[cfg(feature = "parquet")]
use crate::config::state::parquet::Schema;

#[cfg(feature = "parquet")]
use super::parquet;

enum StateWriterType {
    Json {
        buffer: StateConfig,
        state_file_path: PathBuf,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        coins: parquet::Encoder<std::fs::File, CoinConfig>,
        messages: parquet::Encoder<std::fs::File, MessageConfig>,
        contracts: parquet::Encoder<std::fs::File, ContractConfig>,
        contract_state: parquet::Encoder<std::fs::File, ContractStateConfig>,
        contract_balance: parquet::Encoder<std::fs::File, ContractBalanceConfig>,
    },
}

pub struct StateWriter {
    encoder: StateWriterType,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg(feature = "parquet")]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum ZstdCompressionLevel {
    Uncompressed,
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
    Level6,
    Level7,
    Level8,
    Level9,
    Level10,
    Level11,
    Level12,
    Level13,
    Level14,
    Level15,
    Level16,
    Level17,
    Level18,
    Level19,
    Level20,
    Level21,
    Max,
}

#[cfg(feature = "parquet")]
impl TryFrom<u8> for ZstdCompressionLevel {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Uncompressed),
            1 => Ok(Self::Level1),
            2 => Ok(Self::Level2),
            3 => Ok(Self::Level3),
            4 => Ok(Self::Level4),
            5 => Ok(Self::Level5),
            6 => Ok(Self::Level6),
            7 => Ok(Self::Level7),
            8 => Ok(Self::Level8),
            9 => Ok(Self::Level9),
            10 => Ok(Self::Level10),
            11 => Ok(Self::Level11),
            12 => Ok(Self::Level12),
            13 => Ok(Self::Level13),
            14 => Ok(Self::Level14),
            15 => Ok(Self::Level15),
            16 => Ok(Self::Level16),
            17 => Ok(Self::Level17),
            18 => Ok(Self::Level18),
            19 => Ok(Self::Level19),
            20 => Ok(Self::Level20),
            21 => Ok(Self::Level21),
            22 => Ok(Self::Max),
            _ => {
                anyhow::bail!("Compression level {value} outside of allowed range 0..=22")
            }
        }
    }
}

#[cfg(feature = "parquet")]
impl From<ZstdCompressionLevel> for u8 {
    fn from(value: ZstdCompressionLevel) -> Self {
        match value {
            ZstdCompressionLevel::Uncompressed => 0,
            ZstdCompressionLevel::Level1 => 1,
            ZstdCompressionLevel::Level2 => 2,
            ZstdCompressionLevel::Level3 => 3,
            ZstdCompressionLevel::Level4 => 4,
            ZstdCompressionLevel::Level5 => 5,
            ZstdCompressionLevel::Level6 => 6,
            ZstdCompressionLevel::Level7 => 7,
            ZstdCompressionLevel::Level8 => 8,
            ZstdCompressionLevel::Level9 => 9,
            ZstdCompressionLevel::Level10 => 10,
            ZstdCompressionLevel::Level11 => 11,
            ZstdCompressionLevel::Level12 => 12,
            ZstdCompressionLevel::Level13 => 13,
            ZstdCompressionLevel::Level14 => 14,
            ZstdCompressionLevel::Level15 => 15,
            ZstdCompressionLevel::Level16 => 16,
            ZstdCompressionLevel::Level17 => 17,
            ZstdCompressionLevel::Level18 => 18,
            ZstdCompressionLevel::Level19 => 19,
            ZstdCompressionLevel::Level20 => 20,
            ZstdCompressionLevel::Level21 => 21,
            ZstdCompressionLevel::Max => 22,
        }
    }
}

#[cfg(feature = "parquet")]
impl From<ZstdCompressionLevel> for ::parquet::basic::Compression {
    fn from(value: ZstdCompressionLevel) -> Self {
        if let ZstdCompressionLevel::Uncompressed = value {
            Self::UNCOMPRESSED
        } else {
            let level = i32::from(u8::from(value));
            let level = ::parquet::basic::ZstdLevel::try_new(level)
                .expect("our range to mimic the parquet zstd range");
            Self::ZSTD(level)
        }
    }
}

impl StateWriter {
    pub fn json(snapshot_dir: impl AsRef<Path>) -> Self {
        Self {
            encoder: StateWriterType::Json {
                state_file_path: snapshot_dir.as_ref().join(crate::STATE_CONFIG_FILENAME),
                buffer: StateConfig::default(),
            },
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(
        snapshot_dir: impl AsRef<Path>,
        compression_level: ZstdCompressionLevel,
    ) -> anyhow::Result<Self> {
        use ::parquet::basic::Compression;
        use std::fs::File;

        fn create_encoder<T>(
            path: &Path,
            name: &str,
            compression: Compression,
        ) -> anyhow::Result<parquet::Encoder<File, T>>
        where
            T: Schema,
        {
            use anyhow::Context;
            let path = path.join(format!("{name}.parquet"));
            let file = std::fs::File::create(&path)
                .with_context(|| format!("Cannot open file ({path:?}) for writing"))?;
            parquet::Encoder::new(file, compression)
        }

        let path = snapshot_dir.as_ref();
        let compression = compression_level.into();

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
            #[cfg(feature = "parquet")]
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
            #[cfg(feature = "parquet")]
            StateWriterType::Parquet { contracts, .. } => contracts.write(elements),
        }
    }

    pub fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        match &mut self.encoder {
            StateWriterType::Json { buffer: state, .. } => {
                state.messages.extend(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
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
            #[cfg(feature = "parquet")]
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
            #[cfg(feature = "parquet")]
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
            #[cfg(feature = "parquet")]
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

#[cfg(feature = "random")]
#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;

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

    #[cfg(feature = "parquet")]
    #[test]
    fn parquet_encoder_generates_expected_filenames() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder =
            StateWriter::parquet(dir.path(), ZstdCompressionLevel::Uncompressed).unwrap();

        // when
        encoder.close().unwrap();

        // then
        let entries: std::collections::HashSet<_> = dir
            .path()
            .read_dir()
            .unwrap()
            .map_ok(|entry| entry.path())
            .try_collect()
            .unwrap();
        let expected_files = std::collections::HashSet::from(
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

    #[cfg(feature = "parquet")]
    #[test]
    fn parquet_encoder_encodes_types_in_expected_files() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

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

    #[cfg(feature = "parquet")]
    fn test_data_written_in_expected_file<T>(
        rng: impl rand::Rng,
        expected_filename: &str,
        write: impl FnOnce(Vec<T>, &mut StateWriter) -> anyhow::Result<()>,
    ) where
        parquet::Decoder<std::fs::File, T>:
            Iterator<Item = anyhow::Result<crate::Group<T>>>,
        T: crate::Randomize + PartialEq + ::core::fmt::Debug + Clone,
    {
        use std::fs::File;
        // given
        let dir = tempfile::tempdir().unwrap();
        let mut encoder =
            StateWriter::parquet(dir.path(), ZstdCompressionLevel::Uncompressed).unwrap();
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

    #[cfg(feature = "parquet")]
    #[test]
    fn all_compressions_are_valid() {
        use ::parquet::basic::Compression;
        use strum::IntoEnumIterator;
        for level in ZstdCompressionLevel::iter() {
            let _ = Compression::from(level);
        }
    }
}
