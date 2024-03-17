use crate::{
    config::{
        contract_state::ContractStateConfig,
        my_entry::MyEntry,
    },
    AsTable,
    ChainConfig,
    CoinConfig,
    ContractBalanceConfig,
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

    pub fn write(
        mut self,
        state_config: StateConfig,
    ) -> anyhow::Result<SnapshotMetadata> {
        self.write_coins(state_config.as_table())?;
        self.write_contracts_code(state_config.as_table())?;
        self.write_contracts_info(state_config.as_table())?;
        self.write_contracts_utxos(state_config.as_table())?;
        self.write_messages(state_config.as_table())?;
        self.write_contract_state(state_config.as_table())?;
        self.write_contract_balance(state_config.as_table())?;
        self.write_block_data(state_config.block_height, state_config.da_block_height)?;
        self.close()
    }

    pub fn write_chain_config(
        &mut self,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        chain_config.write(self.dir.join("chain_config.json"))
    }

    pub fn write_coins(&mut self, elements: Vec<MyEntry<Coins>>) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn wrt<T>(&mut self, elements: Vec<MyEntry<T>>) -> anyhow::Result<()>
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

    pub fn write_contracts_code(
        &mut self,
        elements: Vec<MyEntry<ContractsRawCode>>,
    ) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn write_contracts_info(
        &mut self,
        elements: Vec<MyEntry<ContractsInfo>>,
    ) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn write_contracts_utxos(
        &mut self,
        elements: Vec<MyEntry<ContractsLatestUtxo>>,
    ) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn write_messages(
        &mut self,
        elements: Vec<MyEntry<Messages>>,
    ) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn write_contract_state(
        &mut self,
        elements: Vec<MyEntry<ContractsState>>,
    ) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn write_contract_balance(
        &mut self,
        elements: Vec<MyEntry<ContractsAssets>>,
    ) -> anyhow::Result<()> {
        self.wrt(elements)
    }

    pub fn write_block_data(
        &mut self,
        height: BlockHeight,
        da_height: DaBlockHeight,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn close(self) -> anyhow::Result<SnapshotMetadata> {
        match self.encoder {
            EncoderType::Json { buffer } => {
                let state_file_path = self.dir.join("state_config.json");

                let file = std::fs::File::create(&state_file_path)?;
                let mut map = serde_json::map::Map::new();
                for (k, v) in buffer {
                    map.insert(k, serde_json::Value::Array(v));
                }

                serde_json::to_writer_pretty(file, &map)?;

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
        metadata.set_parent_dir(dir);
        metadata.write(dir)?;
        Ok(metadata)
    }
}

// #[cfg(feature = "random")]
// #[cfg(test)]
// mod tests {
//     use fuel_core_types::{
//         blockchain::primitives::DaBlockHeight,
//         fuel_types::{
//             BlockHeight,
//             Nonce,
//         },
//     };
//
//     use super::*;
//     use itertools::Itertools;
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn can_roundtrip_compression_level() {
//         use strum::IntoEnumIterator;
//
//         for level in crate::ZstdCompressionLevel::iter() {
//             let u8_level = u8::from(level);
//             let roundtrip = ZstdCompressionLevel::try_from(u8_level).unwrap();
//             assert_eq!(level, roundtrip);
//         }
//     }
//
//     #[test]
//     fn json_encoder_generates_single_file_with_expected_name() {
//         // given
//         let dir = tempfile::tempdir().unwrap();
//         let file = dir.path().join("state_config.json");
//         let encoder = SnapshotWriter::json(&file);
//
//         // when
//         encoder.close().unwrap();
//
//         // then
//         let entries: Vec<_> = dir.path().read_dir().unwrap().try_collect().unwrap();
//
//         match entries.as_slice() {
//             [entry] => assert_eq!(entry.path(), file),
//             _ => panic!("Expected single file \"state_config.json\""),
//         }
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn parquet_encoder_generates_expected_filenames() {
//         // given
//         let dir = tempfile::tempdir().unwrap();
//         let encoder =
//             SnapshotWriter::parquet(dir.path(), ZstdCompressionLevel::Uncompressed)
//                 .unwrap();
//
//         // when
//         encoder.close().unwrap();
//
//         // then
//         let entries: std::collections::HashSet<_> = dir
//             .path()
//             .read_dir()
//             .unwrap()
//             .map_ok(|entry| entry.path())
//             .try_collect()
//             .unwrap();
//         let expected_files = std::collections::HashSet::from(
//             [
//                 "coins.parquet",
//                 "messages.parquet",
//                 "contracts.parquet",
//                 "contract_state.parquet",
//                 "contract_balance.parquet",
//                 "block_height.parquet",
//                 "da_block_height.parquet",
//             ]
//             .map(|name| dir.path().join(name)),
//         );
//
//         assert_eq!(entries, expected_files);
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn parquet_encoder_encodes_types_in_expected_files() {
//         use rand::SeedableRng;
//         let mut rng = rand::rngs::StdRng::seed_from_u64(0);
//
//         test_data_written_in_expected_file::<CoinConfig>(
//             &mut rng,
//             "coins.parquet",
//             |coins, encoder| encoder.write_coins(coins),
//         );
//
//         test_data_written_in_expected_file::<MessageConfig>(
//             &mut rng,
//             "messages.parquet",
//             |coins, encoder| encoder.write_messages(coins),
//         );
//
//         test_data_written_in_expected_file::<ContractConfig>(
//             &mut rng,
//             "contracts.parquet",
//             |coins, encoder| encoder.write_contracts(coins),
//         );
//
//         test_data_written_in_expected_file::<ContractStateConfig>(
//             &mut rng,
//             "contract_state.parquet",
//             |coins, encoder| encoder.write_contract_state(coins),
//         );
//
//         test_data_written_in_expected_file::<ContractBalanceConfig>(
//             &mut rng,
//             "contract_balance.parquet",
//             |coins, encoder| encoder.write_contract_balance(coins),
//         );
//     }
//
//     #[cfg(feature = "parquet")]
//     fn test_data_written_in_expected_file<T>(
//         rng: impl rand::Rng,
//         expected_filename: &str,
//         write: impl FnOnce(Vec<T>, &mut SnapshotWriter) -> anyhow::Result<()>,
//     ) where
//         parquet::decode::Decoder<std::fs::File>:
//             Iterator<Item = anyhow::Result<crate::Group<T>>>,
//         T: crate::Randomize + PartialEq + ::core::fmt::Debug + Clone,
//     {
//         // given
//
//         use self::parquet::decode::Decoder;
//         let dir = tempfile::tempdir().unwrap();
//         let mut encoder =
//             SnapshotWriter::parquet(dir.path(), ZstdCompressionLevel::Uncompressed)
//                 .unwrap();
//         let original_data = vec![T::randomize(rng)];
//
//         // when
//         write(original_data.clone(), &mut encoder).unwrap();
//         encoder.close().unwrap();
//
//         // then
//         let file = std::fs::File::open(dir.path().join(expected_filename)).unwrap();
//         let decoded = parquet::decode::PostcardDecoder::<T>::new(file)
//             .unwrap()
//             .collect::<Result<Vec<_>, _>>()
//             .unwrap();
//         assert_eq!(original_data, decoded.first().unwrap().data);
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn all_compressions_are_valid() {
//         use ::parquet::basic::Compression;
//         use strum::IntoEnumIterator;
//         for level in ZstdCompressionLevel::iter() {
//             let _ = Compression::from(level);
//         }
//     }
//
//     #[test]
//     fn json_coins_are_human_readable() {
//         // given
//         let dir = tempfile::tempdir().unwrap();
//         let filepath = dir.path().join("some_file.json");
//         let mut encoder = SnapshotWriter::json(&filepath);
//         let coin = CoinConfig {
//             tx_id: [1u8; 32].into(),
//             output_index: 2,
//             tx_pointer_block_height: BlockHeight::new(3),
//             tx_pointer_tx_idx: 4,
//             owner: [6u8; 32].into(),
//             amount: 7,
//             asset_id: [8u8; 32].into(),
//         };
//
//         // when
//         encoder.write_coins(vec![coin.clone()]).unwrap();
//         encoder.close().unwrap();
//
//         // then
//         let encoded_json = std::fs::read_to_string(&filepath).unwrap();
//
//         insta::assert_snapshot!(encoded_json);
//     }
//
//     #[test]
//     fn json_messages_are_human_readable() {
//         // given
//         let dir = tempfile::tempdir().unwrap();
//         let filepath = dir.path().join("some_file.json");
//         let mut encoder = SnapshotWriter::json(&filepath);
//         let message = MessageConfig {
//             sender: [1u8; 32].into(),
//             recipient: [2u8; 32].into(),
//             nonce: Nonce::new([3u8; 32]),
//             amount: 4,
//             data: [5u8; 32].into(),
//             da_height: DaBlockHeight(6),
//         };
//
//         // when
//         encoder.write_messages(vec![message.clone()]).unwrap();
//         encoder.close().unwrap();
//
//         // then
//         let encoded_json = std::fs::read_to_string(filepath).unwrap();
//
//         insta::assert_snapshot!(encoded_json);
//     }
//
//     #[test]
//     fn json_contracts_are_human_readable() {
//         // given
//         let dir = tempfile::tempdir().unwrap();
//         let filepath = dir.path().join("some_file.json");
//         let mut encoder = SnapshotWriter::json(&filepath);
//         let contract = ContractConfig {
//             contract_id: [1u8; 32].into(),
//             code: [2u8; 32].into(),
//             salt: [3u8; 32].into(),
//             tx_id: [4u8; 32].into(),
//             output_index: 5,
//             tx_pointer_block_height: BlockHeight::new(6),
//             tx_pointer_tx_idx: 7,
//         };
//
//         // when
//         encoder.write_contracts(vec![contract.clone()]).unwrap();
//         encoder.close().unwrap();
//
//         // then
//         let encoded_json = std::fs::read_to_string(filepath).unwrap();
//         insta::assert_snapshot!(encoded_json);
//     }
// }
