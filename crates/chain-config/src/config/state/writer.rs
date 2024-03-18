use crate::{
    config::my_entry::MyEntry,
    AsTable,
    ChainConfig,
    CoinConfig,
    ContractConfig,
    MessageConfig,
    SnapshotMetadata,
    StateConfig,
    TableEncoding,
};
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
    Mappable,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::coins::coin::CompressedCoin,
    fuel_tx::UtxoId,
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    path::PathBuf,
};

#[cfg(feature = "parquet")]
use super::parquet;

enum EncoderType {
    Json {
        buffer: HashMap<String, Vec<Value>>,
        da_block_height: Option<DaBlockHeight>,
        block_height: Option<BlockHeight>,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        compression: ZstdCompressionLevel,
        table_encoders:
            HashMap<String, (PathBuf, parquet::encode::Encoder<std::fs::File>)>,
        block_height: PathBuf,
        da_block_height: PathBuf,
    },
}

pub struct SnapshotWriter {
    dir: PathBuf,
    encoder: EncoderType,
}

#[allow(dead_code)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
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

impl<T> MyEntry<T>
where
    T: Mappable,
    T::OwnedValue: serde::Serialize,
    T::OwnedKey: serde::Serialize,
{
    #[cfg(feature = "parquet")]
    pub fn encode_postcard(&self) -> Vec<u8> {
        postcard::to_stdvec(self).unwrap()
    }

    pub fn encode_json(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

impl SnapshotWriter {
    pub fn json(dir: impl Into<PathBuf>) -> Self {
        Self {
            encoder: EncoderType::Json {
                buffer: HashMap::default(),
                da_block_height: None,
                block_height: None,
            },
            dir: dir.into(),
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(
        dir: impl Into<::std::path::PathBuf>,
        compression_level: ZstdCompressionLevel,
    ) -> anyhow::Result<Self> {
        let dir = dir.into();
        Ok(Self {
            encoder: EncoderType::Parquet {
                table_encoders: HashMap::default(),
                compression: compression_level,
                block_height: dir.join("block_height.parquet"),
                da_block_height: dir.join("da_block_height.parquet"),
            },
            dir,
        })
    }

    pub fn write_state_config(
        mut self,
        state_config: StateConfig,
    ) -> anyhow::Result<SnapshotMetadata> {
        self.write::<Coins>(state_config.as_table())?;
        self.write::<ContractsRawCode>(state_config.as_table())?;
        self.write::<ContractsInfo>(state_config.as_table())?;
        self.write::<ContractsLatestUtxo>(state_config.as_table())?;
        self.write::<Messages>(state_config.as_table())?;
        self.write::<ContractsState>(state_config.as_table())?;
        self.write::<ContractsAssets>(state_config.as_table())?;
        self.write_block_data(state_config.block_height, state_config.da_block_height)?;
        self.close()
    }

    pub fn write_chain_config(
        &mut self,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        chain_config.write(self.dir.join("chain_config.json"))
    }

    pub fn write<T>(&mut self, elements: Vec<MyEntry<T>>) -> anyhow::Result<()>
    where
        T: TableWithBlueprint,
        T::OwnedValue: serde::Serialize,
        T::OwnedKey: serde::Serialize,
    {
        let name = T::column().name().to_string();
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                let values = elements.into_iter().map(|e| e.encode_json()).collect_vec();
                state.entry(name).or_insert_with(Vec::new).extend(values);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                compression,
                table_encoders,
                ..
            } => {
                let encoded = elements.into_iter().map(|e| e.encode_postcard()).collect();
                let file_path = self.dir.join(format!("{name}.parquet"));
                let (_, encoder) =
                    table_encoders.entry(name.clone()).or_insert_with(|| {
                        let file = File::create(&file_path).unwrap();
                        (
                            file_path,
                            parquet::encode::Encoder::new(file, (*compression).into())
                                .unwrap(),
                        )
                    });
                encoder.write(encoded)
            }
        }
    }

    pub fn write_block_data(
        &mut self,
        height: BlockHeight,
        da_height: DaBlockHeight,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json {
                da_block_height,
                block_height,
                ..
            } => {
                *block_height = Some(height);
                *da_block_height = Some(da_height);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                block_height,
                da_block_height,
                compression,
                ..
            } => {
                let dt: u32 = height.into();
                eprintln!("Writing {:?}", dt);
                Self::write_single_el_parquet(block_height, height, *compression)?;
                let dt: u64 = da_height.into();
                eprintln!("Writing {:?}", dt);
                Self::write_single_el_parquet(da_block_height, da_height, *compression)?;

                Ok(())
            }
        }
    }

    #[cfg(feature = "parquet")]
    fn write_single_el_parquet(
        path: &std::path::Path,
        data: impl serde::Serialize,
        compression: ZstdCompressionLevel,
    ) -> anyhow::Result<()> {
        let mut encoder =
            parquet::encode::Encoder::new(File::create(path)?, compression.into())?;
        encoder.write(vec![postcard::to_stdvec(&data)?])?;
        encoder.close()
    }

    pub fn close(self) -> anyhow::Result<SnapshotMetadata> {
        match self.encoder {
            EncoderType::Json {
                mut buffer,
                da_block_height,
                block_height,
            } => {
                fn extract_table<T>(
                    buffer: &mut HashMap<String, Vec<Value>>,
                ) -> anyhow::Result<Vec<MyEntry<T>>>
                where
                    T: TableWithBlueprint,
                    T::OwnedKey: serde::de::DeserializeOwned,
                    T::OwnedValue: serde::de::DeserializeOwned,
                {
                    Ok(buffer
                        .remove_entry(T::column().name())
                        .unwrap_or_default()
                        .1
                        .into_iter()
                        .map(serde_json::from_value)
                        .try_collect()?)
                }

                let state_config = StateConfig::from_tables(
                    extract_table(&mut buffer)?,
                    extract_table(&mut buffer)?,
                    extract_table(&mut buffer)?,
                    extract_table(&mut buffer)?,
                    extract_table(&mut buffer)?,
                    extract_table(&mut buffer)?,
                    extract_table(&mut buffer)?,
                    da_block_height.unwrap_or_default(),
                    block_height.unwrap_or_default(),
                );

                let state_file_path = self.dir.join("state_config.json");
                let file = std::fs::File::create(&state_file_path)?;
                serde_json::to_writer_pretty(file, &state_config)?;

                Self::write_metadata(
                    &self.dir,
                    TableEncoding::Json {
                        filepath: state_file_path,
                    },
                )
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                table_encoders,
                block_height,
                da_block_height,
                compression,
            } => {
                let mut files = HashMap::new();
                for (name, (file, encoder)) in table_encoders {
                    encoder.close()?;
                    files.insert(name, file);
                }

                Self::write_metadata(
                    &self.dir,
                    TableEncoding::Parquet {
                        tables: files,
                        block_height,
                        da_block_height,
                        compression,
                    },
                )
            }
        }
    }

    fn write_metadata(
        dir: &std::path::Path,
        table_encoding: TableEncoding,
    ) -> anyhow::Result<SnapshotMetadata> {
        let mut metadata = SnapshotMetadata {
            chain_config: dir.join("chain_config.json"),
            table_encoding,
        };
        metadata.strip_prefix(dir)?;
        metadata.write(dir)?;
        metadata.prepend_path(dir);
        Ok(metadata)
    }
}

#[cfg(feature = "random")]
#[cfg(test)]
mod tests {
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_types::{
            BlockHeight,
            Nonce,
        },
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use super::*;
    use itertools::Itertools;

    #[cfg(feature = "parquet")]
    #[test]
    fn can_roundtrip_compression_level() {
        use strum::IntoEnumIterator;

        for level in crate::ZstdCompressionLevel::iter() {
            let u8_level = u8::from(level);
            let roundtrip = ZstdCompressionLevel::try_from(u8_level).unwrap();
            assert_eq!(level, roundtrip);
        }
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn parquet_encoder_encodes_tables_in_expected_files() {
        fn file_created_and_present_in_metadata<T>()
        where
            T: TableWithBlueprint,
            T::OwnedKey: serde::Serialize,
            T::OwnedValue: serde::Serialize,
        {
            // given
            use pretty_assertions::assert_eq;

            let dir = tempfile::tempdir().unwrap();
            let mut encoder =
                SnapshotWriter::parquet(dir.path(), ZstdCompressionLevel::Uncompressed)
                    .unwrap();

            // when
            encoder.write::<T>(vec![]).unwrap();
            let snapshot = encoder.close().unwrap();

            // then
            let TableEncoding::Parquet { tables, .. } = snapshot.table_encoding else {
                panic!("Expected parquet encoding")
            };
            assert_eq!(tables.len(), 1, "Expected single table");
            let (table_name, path) = tables.into_iter().next().unwrap();

            assert_eq!(table_name, T::column().name());
            assert!(dir.path().join(path).exists());
        }

        file_created_and_present_in_metadata::<Coins>();
        file_created_and_present_in_metadata::<Messages>();
        file_created_and_present_in_metadata::<ContractsRawCode>();
        file_created_and_present_in_metadata::<ContractsInfo>();
        file_created_and_present_in_metadata::<ContractsLatestUtxo>();
        file_created_and_present_in_metadata::<ContractsState>();
        file_created_and_present_in_metadata::<ContractsAssets>();
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

    #[cfg(all(feature = "random", feature = "std"))]
    #[test]
    fn json_snapshot_is_human_readable() {
        // given
        use crate::Randomize;
        let dir = tempfile::tempdir().unwrap();
        let encoder = SnapshotWriter::json(dir.path());
        let mut rng = StdRng::from_seed([0; 32]);
        let state = StateConfig::randomize(&mut rng);

        // when
        let snapshot = encoder.write_state_config(state).unwrap();

        // then
        let TableEncoding::Json { filepath } = snapshot.table_encoding else {
            panic!("Expected json encoding")
        };
        let encoded_json = std::fs::read_to_string(filepath).unwrap();

        insta::assert_snapshot!(encoded_json);
    }
}
