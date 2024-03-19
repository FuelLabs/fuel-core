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

    pub fn strip_prefix(&mut self, dir: &Path) -> anyhow::Result<&mut Self> {
        self.chain_config = self.chain_config.strip_prefix(dir)?.to_owned();
        self.table_encoding.strip_prefix(dir)?;
        Ok(self)
    }

    pub fn prepend_path(&mut self, dir: &Path) {
        self.chain_config = dir.join(&self.chain_config);
        self.table_encoding.prepend_path(dir);
    }

    pub fn write(&self, dir: &Path) -> anyhow::Result<()> {
        let path = dir.join(Self::METADATA_FILENAME);
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod json {
        use super::*;

        #[test]
        fn prepends_paths() {
            // given
            let mut snapshot = SnapshotMetadata {
                chain_config: "chain_config.json".into(),
                table_encoding: TableEncoding::Json {
                    filepath: "state_config.json".into(),
                },
            };

            // when
            snapshot.prepend_path(Path::new("./dir"));

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: PathBuf::from("./dir/chain_config.json"),
                    table_encoding: TableEncoding::Json {
                        filepath: PathBuf::from("./dir/state_config.json"),
                    },
                }
            );
        }

        #[test]
        fn strips_prefix() {
            // given
            let mut snapshot = SnapshotMetadata {
                chain_config: "./dir/chain_config.json".into(),
                table_encoding: TableEncoding::Json {
                    filepath: "./dir/state_config.json".into(),
                },
            };

            // when
            snapshot.strip_prefix(Path::new("./dir")).unwrap();

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: PathBuf::from("chain_config.json"),
                    table_encoding: TableEncoding::Json {
                        filepath: PathBuf::from("state_config.json"),
                    },
                }
            );
        }

        #[test]
        fn reading_a_snapshot_produces_valid_paths() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            let dir = temp_dir.path();
            let snapshot = SnapshotMetadata {
                chain_config: "chain_config.json".into(),
                table_encoding: TableEncoding::Json {
                    filepath: "state_config.json".into(),
                },
            };
            snapshot.write(dir).unwrap();

            // when
            let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: dir.join("chain_config.json"),
                    table_encoding: TableEncoding::Json {
                        filepath: temp_dir.path().join("state_config.json"),
                    }
                }
            );
        }
    }

    #[cfg(feature = "parquet")]
    mod parquet {
        use super::*;

        #[test]
        fn prepends_paths() {
            // given
            let mut snapshot = SnapshotMetadata {
                chain_config: "chain_config.json".into(),
                table_encoding: TableEncoding::Parquet {
                    tables: [
                        ("Coins".to_string(), "Coins.parquet".into()),
                        ("Messages".to_string(), "Messages.parquet".into()),
                    ]
                    .into_iter()
                    .collect(),
                    block_height: "block_height.parquet".into(),
                    da_block_height: "da_block_height.parquet".into(),
                    compression: crate::ZstdCompressionLevel::Max,
                },
            };

            // when
            snapshot.prepend_path(Path::new("./dir"));

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: PathBuf::from("./dir/chain_config.json"),
                    table_encoding: TableEncoding::Parquet {
                        tables: [
                            ("Coins".to_string(), PathBuf::from("./dir/Coins.parquet")),
                            (
                                "Messages".to_string(),
                                PathBuf::from("./dir/Messages.parquet")
                            ),
                        ]
                        .into_iter()
                        .collect(),
                        block_height: PathBuf::from("./dir/block_height.parquet"),
                        da_block_height: PathBuf::from("./dir/da_block_height.parquet"),
                        compression: crate::ZstdCompressionLevel::Max,
                    },
                }
            );
        }

        #[test]
        fn strips_prefix() {
            // given
            let mut snapshot = SnapshotMetadata {
                chain_config: "./dir/chain_config.json".into(),
                table_encoding: TableEncoding::Parquet {
                    tables: [
                        ("Coins".to_string(), "./dir/Coins.parquet".into()),
                        ("Messages".to_string(), "./dir/Messages.parquet".into()),
                    ]
                    .into_iter()
                    .collect(),
                    block_height: "./dir/block_height.parquet".into(),
                    da_block_height: "./dir/da_block_height.parquet".into(),
                    compression: crate::ZstdCompressionLevel::Max,
                },
            };

            // when
            snapshot.strip_prefix(Path::new("./dir")).unwrap();

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: PathBuf::from("chain_config.json"),
                    table_encoding: TableEncoding::Parquet {
                        tables: [
                            ("Coins".to_string(), PathBuf::from("Coins.parquet")),
                            ("Messages".to_string(), PathBuf::from("Messages.parquet")),
                        ]
                        .into_iter()
                        .collect(),
                        block_height: PathBuf::from("block_height.parquet"),
                        da_block_height: PathBuf::from("da_block_height.parquet"),
                        compression: crate::ZstdCompressionLevel::Max,
                    },
                }
            );
        }

        #[test]
        fn reading_a_snapshot_produces_valid_paths() {
            // given
            let temp_dir = tempfile::tempdir().unwrap();
            SnapshotMetadata {
                chain_config: "chain_config.json".into(),
                table_encoding: TableEncoding::Parquet {
                    tables: [
                        ("Coins".to_string(), "Coins.parquet".into()),
                        ("Messages".to_string(), "Messages.parquet".into()),
                    ]
                    .into_iter()
                    .collect(),
                    block_height: "block_height.parquet".into(),
                    da_block_height: "da_block_height.parquet".into(),
                    compression: crate::ZstdCompressionLevel::Max,
                },
            }
            .write(temp_dir.path())
            .unwrap();

            // when
            let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();

            // then
            assert_eq!(
                snapshot,
                SnapshotMetadata {
                    chain_config: temp_dir.path().join("chain_config.json"),
                    table_encoding: TableEncoding::Parquet {
                        tables: [
                            ("Coins".to_string(), temp_dir.path().join("Coins.parquet")),
                            (
                                "Messages".to_string(),
                                temp_dir.path().join("Messages.parquet")
                            ),
                        ]
                        .into_iter()
                        .collect(),
                        block_height: temp_dir.path().join("block_height.parquet"),
                        da_block_height: temp_dir.path().join("da_block_height.parquet"),
                        compression: crate::ZstdCompressionLevel::Max,
                    }
                }
            );
        }
    }
}
