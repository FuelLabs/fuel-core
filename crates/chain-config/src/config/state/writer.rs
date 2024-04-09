use crate::{
    config::table_entry::TableEntry,
    AddTable,
    ChainConfig,
    SnapshotMetadata,
    StateConfigBuilder,
    TableEncoding,
};
use fuel_core_storage::structured_storage::TableWithBlueprint;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use std::path::PathBuf;

#[cfg(feature = "parquet")]
use super::parquet;

enum EncoderType {
    Json {
        builder: StateConfigBuilder,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        compression: ZstdCompressionLevel,
        table_encoders: TableEncoders,
        block_height: Option<PathBuf>,
        da_block_height: Option<PathBuf>,
    },
}

pub struct SnapshotWriter {
    dir: PathBuf,
    chain_config: Option<ChainConfig>,
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

#[derive(Debug, Clone)]
enum FragmentData {
    Json {
        builder: StateConfigBuilder,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        tables: std::collections::HashMap<String, PathBuf>,
        block_height: Option<PathBuf>,
        da_block_height: Option<PathBuf>,
    },
}
impl FragmentData {
    fn merge(mut self, data: FragmentData) -> anyhow::Result<Self> {
        match (&mut self, data) {
            (
                FragmentData::Json { builder, .. },
                FragmentData::Json {
                    builder: other_builder,
                    ..
                },
            ) => {
                builder.merge(other_builder);
            }
            #[cfg(feature = "parquet")]
            (
                FragmentData::Parquet {
                    tables,
                    block_height,
                    da_block_height,
                },
                FragmentData::Parquet {
                    tables: their_tables,
                    block_height: their_block_height,
                    da_block_height: their_da_block_height,
                },
            ) => {
                tables.extend(their_tables);
                if their_block_height.is_some() {
                    *block_height = their_block_height;
                }
                if their_da_block_height.is_some() {
                    *da_block_height = their_da_block_height;
                }
            }
            #[cfg(feature="parquet")]
            (a,b) => anyhow::bail!("Fragments don't have the same encoding and cannot be merged. Fragments: {a:?} and {b:?}"),
        };

        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotFragment {
    chain_config: Option<ChainConfig>,
    dir: PathBuf,
    data: FragmentData,
}

impl SnapshotFragment {
    pub fn merge(mut self, fragment: Self) -> anyhow::Result<Self> {
        if let Some(chain_config) = fragment.chain_config {
            self.chain_config = Some(chain_config);
        }

        self.data = self.data.merge(fragment.data)?;
        Ok(self)
    }

    pub fn finalize(self) -> anyhow::Result<SnapshotMetadata> {
        fn verify_block_heights_available<Height, DaHeight>(
            block_height: Option<Height>,
            da_block_height: Option<DaHeight>,
        ) -> anyhow::Result<(Height, DaHeight)> {
            match (block_height, da_block_height) {
                (Some(block_height), Some(da_block_height)) => {
                    Ok((block_height, da_block_height))
                }
                _ => {
                    anyhow::bail!("Snapshot must contain the block heights. Write them before closing the writer.")
                }
            }
        }

        let table_encoding = match self.data {
            FragmentData::Json { builder } => {
                verify_block_heights_available(
                    builder.block_height(),
                    builder.da_block_height(),
                )?;
                let state_config = builder.build()?;
                std::fs::create_dir_all(&self.dir)?;
                let state_file_path = self.dir.join("state_config.json");
                let file = std::fs::File::create(&state_file_path)?;
                serde_json::to_writer_pretty(file, &state_config)?;

                TableEncoding::Json {
                    filepath: state_file_path,
                }
            }
            #[cfg(feature = "parquet")]
            FragmentData::Parquet {
                tables,
                block_height,
                da_block_height,
            } => {
                let (block_height, da_block_height) =
                    verify_block_heights_available(block_height, da_block_height)?;
                TableEncoding::Parquet {
                    tables,
                    block_height,
                    da_block_height,
                }
            }
        };
        SnapshotWriter::write_chain_config_and_metadata(
            &self.dir,
            self.chain_config.as_ref(),
            table_encoding,
        )
    }
}

impl SnapshotWriter {
    const CHAIN_CONFIG_FILENAME: &'static str = "chain_config.json";
    pub fn json(dir: impl Into<PathBuf>) -> Self {
        Self {
            encoder: EncoderType::Json {
                builder: StateConfigBuilder::default(),
            },
            dir: dir.into(),
            chain_config: None,
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(
        dir: impl Into<::std::path::PathBuf>,
        compression_level: ZstdCompressionLevel,
    ) -> anyhow::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            encoder: EncoderType::Parquet {
                table_encoders: TableEncoders::new(dir.clone(), compression_level),
                compression: compression_level,
                block_height: None,
                da_block_height: None,
            },
            dir,
            chain_config: None,
        })
    }

    #[cfg(feature = "test-helpers")]
    pub fn write_state_config(
        mut self,
        state_config: crate::StateConfig,
    ) -> anyhow::Result<SnapshotMetadata> {
        use fuel_core_storage::tables::{
            Coins,
            ContractsAssets,
            ContractsLatestUtxo,
            ContractsRawCode,
            ContractsState,
            Messages,
        };

        use crate::AsTable;

        self.write::<Coins>(state_config.as_table())?;
        self.write::<Messages>(state_config.as_table())?;
        self.write::<ContractsRawCode>(state_config.as_table())?;
        self.write::<ContractsLatestUtxo>(state_config.as_table())?;
        self.write::<ContractsState>(state_config.as_table())?;
        self.write::<ContractsAssets>(state_config.as_table())?;
        self.write_block_data(state_config.block_height, state_config.da_block_height)?;
        self.close()
    }

    pub fn write_chain_config(&mut self, chain_config: &ChainConfig) {
        self.chain_config = Some(chain_config.clone());
    }

    pub fn write<T>(&mut self, elements: Vec<TableEntry<T>>) -> anyhow::Result<()>
    where
        T: TableWithBlueprint,
        TableEntry<T>: serde::Serialize,
        StateConfigBuilder: AddTable<T>,
    {
        match &mut self.encoder {
            EncoderType::Json { builder, .. } => {
                builder.add(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet { table_encoders, .. } => {
                table_encoders.encoder::<T>()?.write::<T>(elements)
            }
        }
    }

    pub fn write_block_data(
        &mut self,
        height: BlockHeight,
        da_height: DaBlockHeight,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { builder, .. } => {
                builder.set_block_height(height);
                builder.set_da_block_height(da_height);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                block_height,
                da_block_height,
                compression,
                ..
            } => {
                let file = self.dir.join("block_height.parquet");
                Self::write_single_el_parquet(&file, height, *compression)?;
                *block_height = Some(file);

                let file = self.dir.join("da_block_height.parquet");
                Self::write_single_el_parquet(&file, da_height, *compression)?;
                *da_block_height = Some(file);

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
        let mut encoder = parquet::encode::Encoder::new(
            std::fs::File::create(path)?,
            compression.into(),
        )?;
        encoder.write(vec![postcard::to_stdvec(&data)?])?;
        encoder.close()
    }

    pub fn close(self) -> anyhow::Result<SnapshotMetadata> {
        self.partial_close()?.finalize()
    }

    fn write_chain_config_and_metadata(
        dir: &std::path::Path,
        chain_config: Option<&ChainConfig>,
        table_encoding: TableEncoding,
    ) -> anyhow::Result<SnapshotMetadata> {
        let chain_config_path = dir.join(Self::CHAIN_CONFIG_FILENAME);
        if let Some(chain_config) = chain_config {
            chain_config.write(&chain_config_path)?;
        } else {
            anyhow::bail!("Snapshot isn't valid without a chain config. Write it before closing the writer.")
        }

        let metadata = SnapshotMetadata {
            chain_config: chain_config_path,
            table_encoding,
        };
        metadata.clone().write(dir)?;
        Ok(metadata)
    }

    pub fn partial_close(self) -> anyhow::Result<SnapshotFragment> {
        let data = match self.encoder {
            EncoderType::Json { builder } => FragmentData::Json { builder },
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                table_encoders,
                block_height,
                da_block_height,
                ..
            } => {
                let tables = table_encoders.close()?;
                FragmentData::Parquet {
                    tables,
                    block_height,
                    da_block_height,
                }
            }
        };
        let snapshot_fragment = SnapshotFragment {
            chain_config: self.chain_config,
            dir: self.dir,
            data,
        };
        Ok(snapshot_fragment)
    }
}

#[cfg(feature = "parquet")]
struct PostcardParquetEncoder {
    path: PathBuf,
    encoder: parquet::encode::Encoder<std::fs::File>,
}

#[cfg(feature = "parquet")]
impl PostcardParquetEncoder {
    pub fn new(path: PathBuf, encoder: parquet::encode::Encoder<std::fs::File>) -> Self {
        Self { path, encoder }
    }

    fn write<T>(&mut self, elements: Vec<TableEntry<T>>) -> anyhow::Result<()>
    where
        T: fuel_core_storage::Mappable,
        TableEntry<T>: serde::Serialize,
    {
        use itertools::Itertools;
        let encoded: Vec<_> = elements
            .into_iter()
            .map(|entry| postcard::to_stdvec(&entry))
            .try_collect()?;
        self.encoder.write(encoded)
    }
}

#[cfg(feature = "parquet")]
struct TableEncoders {
    dir: PathBuf,
    compression: ZstdCompressionLevel,
    encoders: std::collections::HashMap<String, PostcardParquetEncoder>,
}

#[cfg(feature = "parquet")]
impl TableEncoders {
    fn new(dir: PathBuf, compression: ZstdCompressionLevel) -> Self {
        Self {
            dir,
            compression,
            encoders: Default::default(),
        }
    }

    fn encoder<T: fuel_core_storage::structured_storage::TableWithBlueprint>(
        &mut self,
    ) -> anyhow::Result<&mut PostcardParquetEncoder> {
        use fuel_core_storage::kv_store::StorageColumn;

        let name = StorageColumn::name(&T::column()).to_string();

        let encoder = match self.encoders.entry(name) {
            std::collections::hash_map::Entry::Occupied(encoder) => encoder.into_mut(),
            std::collections::hash_map::Entry::Vacant(vacant) => {
                let name = vacant.key();
                let file_path = self.dir.join(format!("{name}.parquet"));
                let file = std::fs::File::create(&file_path)?;
                let encoder = PostcardParquetEncoder::new(
                    file_path,
                    parquet::encode::Encoder::new(file, self.compression.into())?,
                );
                vacant.insert(encoder)
            }
        };

        Ok(encoder)
    }

    fn close(self) -> anyhow::Result<std::collections::HashMap<String, PathBuf>> {
        let mut files = std::collections::HashMap::new();
        for (file, encoder) in self.encoders {
            encoder.encoder.close()?;
            files.insert(file, encoder.path);
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fuel_core_storage::{
        kv_store::StorageColumn,
        structured_storage::TableWithBlueprint,
        tables::{
            Coins,
            ContractsAssets,
            ContractsLatestUtxo,
            ContractsRawCode,
            ContractsState,
            Messages,
        },
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use crate::StateConfig;

    use super::*;

    #[test]
    fn can_roundtrip_compression_level() {
        use strum::IntoEnumIterator;

        for level in crate::ZstdCompressionLevel::iter() {
            let u8_level = u8::from(level);
            let roundtrip = ZstdCompressionLevel::try_from(u8_level).unwrap();
            assert_eq!(level, roundtrip);
        }
    }

    #[test]
    fn parquet_encoder_encodes_tables_in_expected_files() {
        fn file_created_and_present_in_metadata<T>()
        where
            T: TableWithBlueprint,
            T::OwnedKey: serde::Serialize,
            T::OwnedValue: serde::Serialize,
            StateConfigBuilder: AddTable<T>,
        {
            // given
            use pretty_assertions::assert_eq;

            let dir = tempfile::tempdir().unwrap();
            let mut writer =
                SnapshotWriter::parquet(dir.path(), ZstdCompressionLevel::Uncompressed)
                    .unwrap();
            writer.write_chain_config(&ChainConfig::local_testnet());
            writer
                .write_block_data(10.into(), DaBlockHeight(11))
                .unwrap();

            // when
            writer.write::<T>(vec![]).unwrap();
            let snapshot = writer.close().unwrap();

            // then
            assert!(snapshot.chain_config.exists());
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
        file_created_and_present_in_metadata::<ContractsLatestUtxo>();
        file_created_and_present_in_metadata::<ContractsState>();
        file_created_and_present_in_metadata::<ContractsAssets>();
    }

    #[test]
    fn all_compressions_are_valid() {
        use ::parquet::basic::Compression;
        use strum::IntoEnumIterator;
        for level in ZstdCompressionLevel::iter() {
            let _ = Compression::from(level);
        }
    }

    #[test]
    fn json_snapshot_is_human_readable() {
        // given
        use crate::Randomize;
        let dir = tempfile::tempdir().unwrap();
        let mut writer = SnapshotWriter::json(dir.path());
        let mut rng = StdRng::from_seed([0; 32]);
        let state = StateConfig::randomize(&mut rng);
        writer.write_chain_config(&ChainConfig::local_testnet());

        // when
        let snapshot = writer.write_state_config(state).unwrap();

        // then
        let TableEncoding::Json { filepath } = snapshot.table_encoding else {
            panic!("Expected json encoding")
        };
        let encoded_json = std::fs::read_to_string(filepath).unwrap();

        insta::assert_snapshot!(encoded_json);
    }

    fn given_parquet_writer(path: &Path) -> SnapshotWriter {
        SnapshotWriter::parquet(path, ZstdCompressionLevel::Uncompressed).unwrap()
    }

    fn given_json_writer(path: &Path) -> SnapshotWriter {
        SnapshotWriter::json(path)
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn cannot_close_without_chain_config(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let dir = tempfile::tempdir().unwrap();
        let mut writer = writer(dir.path());
        writer
            .write_block_data(10.into(), DaBlockHeight(10))
            .unwrap();

        // when
        let result = writer.close();

        // then
        let err = result.unwrap_err();
        assert_eq!(err.to_string(), "Snapshot isn't valid without a chain config. Write it before closing the writer.");
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn can_partially_close_without_chain_and_block_height(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let dir = tempfile::tempdir().unwrap();
        let writer = writer(dir.path());

        // when
        let result = writer.partial_close();

        // then
        assert!(result.is_ok());
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn cannot_close_without_block_heights(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let dir = tempfile::tempdir().unwrap();
        let mut writer = writer(dir.path());
        writer.write_chain_config(&super::ChainConfig::local_testnet());

        // when
        let result = writer.close();

        // then
        let err = result.unwrap_err();
        assert_eq!(err.to_string(), "Snapshot must contain the block heights. Write them before closing the writer.");
    }

    #[test]
    fn merging_json_and_parquet_fragments_fails() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let json_writer = super::SnapshotWriter::json(dir.path());
        let parquet_writer = super::SnapshotWriter::parquet(
            dir.path(),
            super::ZstdCompressionLevel::Uncompressed,
        );

        let json_fragment = json_writer.partial_close().unwrap();
        let parquet_fragment = parquet_writer.unwrap().partial_close().unwrap();

        {
            // when
            let result = json_fragment.clone().merge(parquet_fragment.clone());

            // when
            let err = result.unwrap_err();
            assert!(err.to_string().contains(
                "Fragments don't have the same encoding and cannot be merged."
            ));
        }
        {
            // when
            let result = parquet_fragment.merge(json_fragment);

            // when
            let err = result.unwrap_err();
            assert!(err.to_string().contains(
                "Fragments don't have the same encoding and cannot be merged."
            ));
        }
    }

    // It is enough just for the test to compile
    #[test]
    #[ignore]
    fn fragment_must_be_send_sync() {
        fn _assert_send<T: Send + Sync>() {}
        _assert_send::<super::SnapshotFragment>();
    }
}
