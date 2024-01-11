use std::path::{
    Path,
    PathBuf,
};

use anyhow::Context;

use crate::CHAIN_CONFIG_FILENAME;

#[cfg(feature = "parquet")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParquetFiles {
    pub coins: PathBuf,
    pub messages: PathBuf,
    pub contracts: PathBuf,
    pub contract_state: PathBuf,
    pub contract_balance: PathBuf,
}

#[cfg(feature = "parquet")]
impl ParquetFiles {
    pub fn snapshot_default(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let parquet_file = |name| dir.join(format!("{name}.parquet"));
        Self {
            coins: parquet_file("coins"),
            messages: parquet_file("messages"),
            contracts: parquet_file("contracts"),
            contract_state: parquet_file("contract_state"),
            contract_balance: parquet_file("contract_balance"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EncodingMeta {
    Json {
        filepath: PathBuf,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        filepaths: ParquetFiles,
        compression: crate::ZstdCompressionLevel,
        group_size: usize,
    },
}

impl EncodingMeta {
    pub fn json(dir: impl AsRef<Path>) -> Self {
        Self::Json {
            filepath: dir.as_ref().join("state_config.json"),
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(
        dir: impl AsRef<Path>,
        compression: crate::ZstdCompressionLevel,
        group_size: usize,
    ) -> Self {
        Self::Parquet {
            filepaths: ParquetFiles::snapshot_default(dir),
            compression,
            group_size,
        }
    }

    pub fn group_size(&self) -> Option<usize> {
        match self {
            #[cfg(feature = "parquet")]
            Self::Parquet { group_size, .. } => Some(*group_size),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotMetadata {
    pub chain_config: PathBuf,
    pub encoding: EncodingMeta,
}

impl SnapshotMetadata {
    pub fn json(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            chain_config: dir.join(CHAIN_CONFIG_FILENAME),
            encoding: EncodingMeta::json(dir),
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(
        dir: impl AsRef<Path>,
        compression: crate::ZstdCompressionLevel,
        group_size: usize,
    ) -> Self {
        let dir = dir.as_ref();
        Self {
            chain_config: dir.join(CHAIN_CONFIG_FILENAME),
            encoding: EncodingMeta::parquet(dir, compression, group_size),
        }
    }

    const METADATA_FILENAME: &'static str = "metadata.json";
    pub fn read_from_dir(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let metadata_path = dir.as_ref().join(Self::METADATA_FILENAME);
        let file = std::fs::File::open(&metadata_path).with_context(|| {
            format!("While opening the snapshot metadata file {metadata_path:?}")
        })?;

        Ok(serde_json::from_reader(file)?)
    }

    pub fn write_to_dir(&self, dir: impl AsRef<Path>) -> anyhow::Result<()> {
        let metadata_path = dir.as_ref().join(Self::METADATA_FILENAME);
        let file = std::fs::File::create(&metadata_path).with_context(|| {
            format!("While creating the snapshot metadata file {metadata_path:?}")
        })?;
        Ok(serde_json::to_writer_pretty(file, &self)?)
    }
}
