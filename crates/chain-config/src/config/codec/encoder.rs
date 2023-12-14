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

use crate::{
    config::{
        codec::parquet::Schema,
        contract_balance::ContractBalance,
        contract_state::ContractStateConfig,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

use super::parquet;

enum EncoderType {
    Json {
        buffer: StateConfig,
        state_file_path: PathBuf,
    },
    Parquet {
        coins: parquet::Encoder<File, CoinConfig>,
        messages: parquet::Encoder<File, MessageConfig>,
        contracts: parquet::Encoder<File, ContractConfig>,
        contract_state: parquet::Encoder<File, ContractStateConfig>,
        contract_balance: parquet::Encoder<File, ContractBalance>,
    },
}

pub struct Encoder {
    encoder: EncoderType,
}

impl Encoder {
    pub fn json(snapshot_dir: impl AsRef<Path>) -> Self {
        Self {
            encoder: EncoderType::Json {
                state_file_path: snapshot_dir.as_ref().join("state.json"),
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
            let file = std::fs::File::create(path)?;
            parquet::Encoder::new(file, compression)
        }

        let path = snapshot_dir.as_ref();
        let compression =
            Compression::GZIP(GzipLevel::try_new(u32::from(compression_level))?);
        Ok(Self {
            encoder: EncoderType::Parquet {
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
            EncoderType::Json { buffer: state, .. } => {
                state.coins.extend(elements);
                Ok(())
            }
            EncoderType::Parquet { coins, .. } => coins.write(elements),
        }
    }

    pub fn write_contracts(
        &mut self,
        elements: Vec<ContractConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.contracts.extend(elements);
                Ok(())
            }
            EncoderType::Parquet { contracts, .. } => contracts.write(elements),
        }
    }

    pub fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.messages.extend(elements);
                Ok(())
            }
            EncoderType::Parquet { messages, .. } => messages.write(elements),
        }
    }

    pub fn write_contract_state(
        &mut self,
        elements: Vec<ContractStateConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.contract_state.extend(elements);
                Ok(())
            }
            EncoderType::Parquet { contract_state, .. } => contract_state.write(elements),
        }
    }

    pub fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.contract_balance.extend(elements);
                Ok(())
            }
            EncoderType::Parquet {
                contract_balance, ..
            } => contract_balance.write(elements),
        }
    }

    pub fn close(self) -> anyhow::Result<()> {
        match self.encoder {
            EncoderType::Json {
                buffer,
                state_file_path,
            } => {
                let file = std::fs::File::create(state_file_path)?;
                serde_json::to_writer(file, &buffer)?;
                Ok(())
            }
            EncoderType::Parquet {
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
    use std::collections::HashSet;

    use crate::Randomize;

    use super::*;
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    #[test]
    fn json_encoder_generates_single_file_with_expected_name() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder = Encoder::json(dir.path());

        // when
        encoder.close().unwrap();

        // then
        let entries: Vec<_> = dir.path().read_dir().unwrap().try_collect().unwrap();

        match entries.as_slice() {
            [entry] => assert_eq!(entry.path(), dir.path().join("state.json")),
            _ => panic!("Expected single file \"state.json\""),
        }
    }

    #[test]
    fn parquet_encoder_generates_expected_filenames() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder = Encoder::parquet(dir.path(), 0).unwrap();

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
    fn parquet_encoder_encodes_coins_in_expected_file() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let mut encoder = Encoder::parquet(dir.path(), 0).unwrap();

        let seeded_rng = StdRng::seed_from_u64(0);

        // when
        let coins = vec![CoinConfig::randomize(seeded_rng)];
        encoder.write_coins(coins.clone()).unwrap();
        encoder.close().unwrap();

        // then
        let coins = parquet::Decoder::<File, CoinConfig>::new(
            dir.path().join("coins.parquet"),
            Compression::GZIP(GzipLevel::try_new(0).unwrap()),
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(coins, coins);
    }
}
