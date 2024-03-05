use std::path::{
    Path,
    PathBuf,
};

use crate::CHAIN_CONFIG_FILENAME;

#[cfg(feature = "parquet")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ParquetFiles {
    pub coins: PathBuf,
    pub messages: PathBuf,
    pub contracts: PathBuf,
    pub contract_state: PathBuf,
    pub contract_balance: PathBuf,
    pub block_height: PathBuf,
}

#[cfg(feature = "parquet")]
impl ParquetFiles {
    fn prepend_relative_paths(&mut self, dir: &Path) {
        let prepend = |path: &mut PathBuf| {
            if path.is_relative() {
                *path = dir.join(&path);
            }
        };
        prepend(&mut self.coins);
        prepend(&mut self.messages);
        prepend(&mut self.contracts);
        prepend(&mut self.contract_state);
        prepend(&mut self.contract_balance);
        prepend(&mut self.block_height);
    }
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
            block_height: parquet_file("block_height"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum StateEncoding {
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

impl StateEncoding {
    pub fn group_size(&self) -> Option<usize> {
        match self {
            #[cfg(feature = "parquet")]
            Self::Parquet { group_size, .. } => Some(*group_size),
            _ => None,
        }
    }
    fn prepend_relative_paths(&mut self, dir: &Path) {
        match self {
            StateEncoding::Json { ref mut filepath } => {
                if filepath.is_relative() {
                    *filepath = dir.join(&filepath);
                }
            }
            #[cfg(feature = "parquet")]
            StateEncoding::Parquet { filepaths, .. } => {
                filepaths.prepend_relative_paths(dir)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotMetadata {
    metadata: Metadata,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct Metadata {
    chain_config: PathBuf,
    state_encoding: StateEncoding,
}
impl Metadata {
    const METADATA_FILENAME: &'static str = "metadata.json";
    fn new(state_encoding: StateEncoding) -> Self {
        Self {
            chain_config: CHAIN_CONFIG_FILENAME.into(),
            state_encoding,
        }
    }
    fn read(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join(Self::METADATA_FILENAME);
        let file = std::fs::File::open(path)?;
        Ok(serde_json::from_reader(&file)?)
    }

    fn write(&self, dir: &Path) -> anyhow::Result<()> {
        let path = dir.join(Self::METADATA_FILENAME);
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    fn prepend_relative_paths(&mut self, dir: &Path) {
        if self.chain_config.is_relative() {
            self.chain_config = dir.join(&self.chain_config);
        }
        self.state_encoding.prepend_relative_paths(dir);
    }
}

impl SnapshotMetadata {
    pub fn chain_config(&self) -> &Path {
        &self.metadata.chain_config
    }

    pub fn state_encoding(&self) -> &StateEncoding {
        &self.metadata.state_encoding
    }

    pub fn take_state_encoding(self) -> StateEncoding {
        self.metadata.state_encoding
    }

    pub fn write_json(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = dir.as_ref();
        let state_encoding = StateEncoding::Json {
            filepath: "state_config.json".into(),
        };
        Self::write(state_encoding, dir)
    }

    #[cfg(feature = "parquet")]
    pub fn write_parquet(
        dir: impl AsRef<Path>,
        compression: crate::ZstdCompressionLevel,
        group_size: usize,
    ) -> anyhow::Result<Self> {
        let dir = dir.as_ref();
        let state_encoding_metadata = StateEncoding::Parquet {
            filepaths: ParquetFiles::snapshot_default("."),
            compression,
            group_size,
        };
        Self::write(state_encoding_metadata, dir)
    }

    fn new(mut metadata: Metadata, dir: &Path) -> Self {
        metadata.prepend_relative_paths(dir);
        Self { metadata }
    }

    fn write(state_encoding: StateEncoding, dir: &Path) -> anyhow::Result<Self> {
        let metadata = Metadata::new(state_encoding);

        metadata.write(dir)?;

        Ok(Self::new(metadata, dir))
    }

    pub fn read(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = dir.as_ref();
        let metadata = Metadata::read(dir)?;

        Ok(Self::new(metadata, dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod json {
        use super::*;

        #[test]
        fn new_snapshot_filepaths_relative_to_given_dir() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();

            // when
            let snapshot = SnapshotMetadata::write_json(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot.chain_config(),
                temp_dir.path().join(CHAIN_CONFIG_FILENAME)
            );
            assert_eq!(
                snapshot.state_encoding(),
                &StateEncoding::Json {
                    filepath: temp_dir.path().join("state_config.json"),
                }
            );
        }

        #[test]
        fn written_filepaths_dont_have_snapshot_dir_in_path() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();

            // when
            SnapshotMetadata::write_json(temp_dir.path()).unwrap();

            // then
            let metadata = Metadata::read(temp_dir.path()).unwrap();
            assert_eq!(metadata.chain_config, PathBuf::from(CHAIN_CONFIG_FILENAME));
            assert_eq!(
                metadata.state_encoding,
                StateEncoding::Json {
                    filepath: PathBuf::from("state_config.json"),
                }
            );
        }

        #[test]
        fn reading_a_snapshot_produces_valid_paths() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            SnapshotMetadata::write_json(temp_dir.path()).unwrap();

            // when
            let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot.chain_config(),
                temp_dir.path().join(CHAIN_CONFIG_FILENAME)
            );
            assert_eq!(
                snapshot.state_encoding(),
                &StateEncoding::Json {
                    filepath: temp_dir.path().join("state_config.json"),
                }
            );
        }

        #[test]
        fn filepaths_leading_outside_snapshot_dir_correctly_resolved() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();

            let snapshot_path = temp_dir.path().join("snapshot");
            std::fs::create_dir_all(&snapshot_path).unwrap();

            let metadata = Metadata {
                chain_config: "../other/chain_config.json".into(),
                state_encoding: StateEncoding::Json {
                    filepath: "../other/state_config.json".into(),
                },
            };
            metadata.write(&snapshot_path).unwrap();

            // when
            let snapshot = SnapshotMetadata::read(&snapshot_path).unwrap();

            // then
            assert_eq!(
                snapshot.chain_config(),
                snapshot_path.join("../other/chain_config.json")
            );
            assert_eq!(
                snapshot.state_encoding(),
                &StateEncoding::Json {
                    filepath: snapshot_path.join("../other/state_config.json"),
                }
            );
        }
    }

    #[cfg(feature = "parquet")]
    mod parquet {
        use super::*;

        #[test]
        fn new_snapshot_filepaths_relative_to_given_dir() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();

            // when
            let snapshot = SnapshotMetadata::write_parquet(
                temp_dir.path(),
                crate::ZstdCompressionLevel::Max,
                10,
            )
            .unwrap();

            // then
            assert_eq!(
                snapshot.chain_config(),
                temp_dir.path().join(CHAIN_CONFIG_FILENAME)
            );
            assert_eq!(
                snapshot.state_encoding(),
                &StateEncoding::Parquet {
                    filepaths: ParquetFiles {
                        coins: temp_dir.path().join("coins.parquet"),
                        messages: temp_dir.path().join("messages.parquet"),
                        contracts: temp_dir.path().join("contracts.parquet"),
                        contract_state: temp_dir.path().join("contract_state.parquet"),
                        contract_balance: temp_dir
                            .path()
                            .join("contract_balance.parquet"),
                        block_height: temp_dir.path().join("block_height.parquet"),
                    },
                    compression: crate::ZstdCompressionLevel::Max,
                    group_size: 10,
                }
            );
        }

        #[test]
        fn written_filepaths_dont_have_snapshot_dir_in_path() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();

            // when
            SnapshotMetadata::write_parquet(
                temp_dir.path(),
                crate::ZstdCompressionLevel::Max,
                10,
            )
            .unwrap();

            // then
            let metadata = Metadata::read(temp_dir.path()).unwrap();
            assert_eq!(metadata.chain_config, PathBuf::from(CHAIN_CONFIG_FILENAME));
            assert_eq!(
                metadata.state_encoding,
                StateEncoding::Parquet {
                    filepaths: ParquetFiles {
                        coins: PathBuf::from("./coins.parquet"),
                        messages: PathBuf::from("./messages.parquet"),
                        contracts: PathBuf::from("./contracts.parquet"),
                        contract_state: PathBuf::from("./contract_state.parquet"),
                        contract_balance: PathBuf::from("./contract_balance.parquet"),
                        block_height: PathBuf::from("./block_height.parquet"),
                    },
                    compression: crate::ZstdCompressionLevel::Max,
                    group_size: 10,
                }
            );
        }

        #[test]
        fn reading_a_snapshot_produces_valid_paths() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            SnapshotMetadata::write_parquet(
                temp_dir.path(),
                crate::ZstdCompressionLevel::Max,
                10,
            )
            .unwrap();

            // when
            let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot.chain_config(),
                temp_dir.path().join(CHAIN_CONFIG_FILENAME)
            );
            assert_eq!(
                snapshot.state_encoding(),
                &StateEncoding::Parquet {
                    filepaths: ParquetFiles {
                        coins: temp_dir.path().join("coins.parquet"),
                        messages: temp_dir.path().join("messages.parquet"),
                        contracts: temp_dir.path().join("contracts.parquet"),
                        contract_state: temp_dir.path().join("contract_state.parquet"),
                        contract_balance: temp_dir
                            .path()
                            .join("contract_balance.parquet"),
                        block_height: temp_dir.path().join("block_height.parquet"),
                    },
                    compression: crate::ZstdCompressionLevel::Max,
                    group_size: 10,
                }
            );
        }

        #[test]
        fn filepaths_leading_outside_snapshot_dir_correctly_resolved() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();

            let snapshot_path = temp_dir.path().join("snapshot");
            std::fs::create_dir_all(&snapshot_path).unwrap();

            let metadata = Metadata {
                chain_config: "../other/chain_config.json".into(),
                state_encoding: StateEncoding::Parquet {
                    filepaths: ParquetFiles {
                        coins: "../other/coins.parquet".into(),
                        messages: "../other/messages.parquet".into(),
                        contracts: "../other/contracts.parquet".into(),
                        contract_state: "../other/contract_state.parquet".into(),
                        contract_balance: "../other/contract_balance.parquet".into(),
                        block_height: "../other/block_height.parquet".into(),
                    },
                    compression: crate::ZstdCompressionLevel::Max,
                    group_size: 10,
                },
            };
            metadata.write(&snapshot_path).unwrap();

            // when
            let snapshot = SnapshotMetadata::read(&snapshot_path).unwrap();

            // then
            assert_eq!(
                snapshot.chain_config(),
                snapshot_path.join("../other/chain_config.json")
            );
            assert_eq!(
                snapshot.state_encoding(),
                &StateEncoding::Parquet {
                    filepaths: ParquetFiles {
                        coins: snapshot_path.join("../other/coins.parquet"),
                        messages: snapshot_path.join("../other/messages.parquet"),
                        contracts: snapshot_path.join("../other/contracts.parquet"),
                        contract_state: snapshot_path
                            .join("../other/contract_state.parquet"),
                        contract_balance: snapshot_path
                            .join("../other/contract_balance.parquet"),
                        block_height: snapshot_path.join("../other/block_height.parquet"),
                    },
                    compression: crate::ZstdCompressionLevel::Max,
                    group_size: 10,
                }
            );
        }
    }
}
