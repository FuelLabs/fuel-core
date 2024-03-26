use std::path::{
    Path,
    PathBuf,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum TableEncoding {
    Json {
        filepath: PathBuf,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        tables: std::collections::HashMap<String, PathBuf>,
        block_height: PathBuf,
        da_block_height: PathBuf,
        compression: crate::ZstdCompressionLevel,
    },
}
impl TableEncoding {
    fn strip_prefix(&mut self, dir: &Path) -> anyhow::Result<()> {
        match self {
            TableEncoding::Json { filepath } => {
                *filepath = filepath.strip_prefix(dir)?.to_owned();
            }
            #[cfg(feature = "parquet")]
            TableEncoding::Parquet {
                tables,
                block_height,
                da_block_height,
                ..
            } => {
                for path in tables.values_mut() {
                    *path = path.strip_prefix(dir)?.to_owned();
                }
                *da_block_height = da_block_height.strip_prefix(dir)?.to_owned();
                *block_height = block_height.strip_prefix(dir)?.to_owned();
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
                block_height,
                da_block_height,
                ..
            } => {
                for path in tables.values_mut() {
                    *path = dir.join(&path);
                }
                *da_block_height = dir.join(&da_block_height);
                *block_height = dir.join(&block_height);
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
        let file = std::fs::File::open(path)?;
        let mut snapshot: Self = serde_json::from_reader(&file)?;
        snapshot.prepend_path(dir.as_ref());

        Ok(snapshot)
    }

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
                    block_height: "block_height.parquet".into(),
                    da_block_height: "da_block_height.parquet".into(),
                    compression: crate::ZstdCompressionLevel::Level1,
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
                        block_height: temp_dir.path().join("block_height.parquet"),
                        da_block_height: temp_dir.path().join("da_block_height.parquet"),
                        compression: crate::ZstdCompressionLevel::Level1,
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
                    block_height: dir.join("block_height.parquet"),
                    da_block_height: dir.join("da_block_height.parquet"),
                    compression: crate::ZstdCompressionLevel::Level1,
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
                        block_height: "block_height.parquet".into(),
                        da_block_height: "da_block_height.parquet".into(),
                        compression: crate::ZstdCompressionLevel::Level1,
                    }
                }
            );
        }
    }
}
