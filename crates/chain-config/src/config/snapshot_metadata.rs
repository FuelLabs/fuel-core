use anyhow::Context;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum TableEncoding {
    Json {
        filepath: PathBuf,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        tables: std::collections::HashMap<String, PathBuf>,
        latest_block_config_path: PathBuf,
    },
}
impl TableEncoding {
    #[allow(clippy::assigning_clones)] // False positive will be fixed in 1.81 Rust (https://github.com/rust-lang/rust-clippy/pull/12756)
    fn strip_prefix(&mut self, dir: &Path) -> anyhow::Result<()> {
        match self {
            TableEncoding::Json { filepath } => {
                *filepath = filepath.strip_prefix(dir)?.to_owned();
            }
            #[cfg(feature = "parquet")]
            TableEncoding::Parquet {
                tables,
                latest_block_config_path,
                ..
            } => {
                for path in tables.values_mut() {
                    *path = path.strip_prefix(dir)?.to_owned();
                }
                *latest_block_config_path =
                    latest_block_config_path.strip_prefix(dir)?.to_owned();
            }
        }
        Ok(())
    }

    fn prepend_path(&mut self, dir: &Path) {
        match self {
            TableEncoding::Json { filepath } => {
                *filepath = dir.join(&filepath);
            }
            #[cfg(feature = "parquet")]
            TableEncoding::Parquet {
                tables,
                latest_block_config_path,
                ..
            } => {
                for path in tables.values_mut() {
                    *path = dir.join(&path);
                }
                *latest_block_config_path = dir.join(&latest_block_config_path);
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SnapshotMetadata {
    pub chain_config: PathBuf,
    pub table_encoding: TableEncoding,
}

impl SnapshotMetadata {
    const METADATA_FILENAME: &'static str = "metadata.json";
    pub fn read(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = dir.as_ref().join(Self::METADATA_FILENAME);
        let mut json = String::new();
        std::fs::File::open(&path)
            .with_context(|| format!("Could not open snapshot file: {path:?}"))?
            .read_to_string(&mut json)?;
        let mut snapshot: Self = serde_json::from_str(json.as_str())?;
        snapshot.prepend_path(dir.as_ref());

        Ok(snapshot)
    }

    #[allow(clippy::assigning_clones)] // False positive will be fixed in 1.81 Rust (https://github.com/rust-lang/rust-clippy/pull/12756)
    fn strip_prefix(&mut self, dir: &Path) -> anyhow::Result<&mut Self> {
        self.chain_config = self.chain_config.strip_prefix(dir)?.to_owned();
        self.table_encoding.strip_prefix(dir)?;
        Ok(self)
    }

    fn prepend_path(&mut self, dir: &Path) {
        self.chain_config = dir.join(&self.chain_config);
        self.table_encoding.prepend_path(dir);
    }

    pub fn write(mut self, dir: &Path) -> anyhow::Result<()> {
        self.strip_prefix(dir)?;
        let path = dir.join(Self::METADATA_FILENAME);
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod json {
        use super::*;

        #[test]
        fn directory_added_to_paths_upon_load() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            let dir = temp_dir.path();
            let data = SnapshotMetadata {
                chain_config: "some_chain_config.json".into(),
                table_encoding: TableEncoding::Json {
                    filepath: "some_state_file.json".into(),
                },
            };
            serde_json::to_writer(
                std::fs::File::create(dir.join("metadata.json")).unwrap(),
                &data,
            )
            .unwrap();

            // when
            let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: dir.join("some_chain_config.json"),
                    table_encoding: TableEncoding::Json {
                        filepath: temp_dir.path().join("some_state_file.json"),
                    }
                }
            );
        }

        #[test]
        fn directory_removed_from_paths_upon_save() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            let dir = temp_dir.path();
            let snapshot = SnapshotMetadata {
                chain_config: dir.join("some_chain_config.json"),
                table_encoding: TableEncoding::Json {
                    filepath: dir.join("some_state_file.json"),
                },
            };

            // when
            snapshot.write(temp_dir.path()).unwrap();

            // then
            let data: SnapshotMetadata = serde_json::from_reader(
                std::fs::File::open(temp_dir.path().join("metadata.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                data,
                SnapshotMetadata {
                    chain_config: "some_chain_config.json".into(),
                    table_encoding: TableEncoding::Json {
                        filepath: "some_state_file.json".into(),
                    }
                }
            );
        }
    }

    #[cfg(feature = "parquet")]
    mod parquet {
        use super::*;
        #[test]
        fn directory_added_to_paths_upon_load() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            let dir = temp_dir.path();
            let data = SnapshotMetadata {
                chain_config: "some_chain_config.json".into(),
                table_encoding: TableEncoding::Parquet {
                    tables: std::collections::HashMap::from_iter(vec![(
                        "coins".into(),
                        "coins.parquet".into(),
                    )]),
                    latest_block_config_path: "latest_block_config.parquet".into(),
                },
            };
            serde_json::to_writer(
                std::fs::File::create(dir.join("metadata.json")).unwrap(),
                &data,
            )
            .unwrap();

            // when
            let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: dir.join("some_chain_config.json"),
                    table_encoding: TableEncoding::Parquet {
                        tables: std::collections::HashMap::from_iter(vec![(
                            "coins".into(),
                            temp_dir.path().join("coins.parquet")
                        )]),
                        latest_block_config_path: temp_dir
                            .path()
                            .join("latest_block_config.parquet"),
                    }
                }
            );
        }

        #[test]
        fn directory_removed_from_paths_upon_save() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            let dir = temp_dir.path();
            let snapshot = SnapshotMetadata {
                chain_config: dir.join("some_chain_config.json"),
                table_encoding: TableEncoding::Parquet {
                    tables: std::collections::HashMap::from_iter([(
                        "coins".into(),
                        dir.join("coins.parquet"),
                    )]),
                    latest_block_config_path: dir.join("latest_block_config.parquet"),
                },
            };

            // when
            snapshot.write(temp_dir.path()).unwrap();

            // then
            let data: SnapshotMetadata = serde_json::from_reader(
                std::fs::File::open(temp_dir.path().join("metadata.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                data,
                SnapshotMetadata {
                    chain_config: "some_chain_config.json".into(),
                    table_encoding: TableEncoding::Parquet {
                        tables: std::collections::HashMap::from_iter([(
                            "coins".into(),
                            "coins.parquet".into(),
                        )]),
                        latest_block_config_path: "latest_block_config.parquet".into(),
                    }
                }
            );
        }
    }
}
