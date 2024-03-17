use std::{
    collections::HashMap,
    path::{
        Path,
        PathBuf,
    },
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum TableEncoding {
    Json {
        filepath: PathBuf,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        tables: HashMap<String, PathBuf>,
        block_height: PathBuf,
        da_block_height: PathBuf,
        compression: crate::ZstdCompressionLevel,
    },
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
        Ok(serde_json::from_reader(&file)?)
    }

    pub fn set_parent_dir(&mut self, dir: &Path) {
        todo!()
    }

    pub fn write(&self, dir: &Path) -> anyhow::Result<()> {
        let path = dir.join(Self::METADATA_FILENAME);
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     mod json {
//         use super::*;
//
//         #[test]
//         fn new_snapshot_filepaths_relative_to_given_dir() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//
//             // when
//             let snapshot = SnapshotMetadata::write_json(temp_dir.path()).unwrap();
//
//             // then
//             assert_eq!(
//                 snapshot.chain_config(),
//                 temp_dir.path().join(CHAIN_CONFIG_FILENAME)
//             );
//             assert_eq!(
//                 snapshot.state_encoding(),
//                 &TableEncoding::Json {
//                     filepath: temp_dir.path().join("state_config.json"),
//                 }
//             );
//         }
//
//         #[test]
//         fn written_filepaths_dont_have_snapshot_dir_in_path() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//
//             // when
//             SnapshotMetadata::write_json(temp_dir.path()).unwrap();
//
//             // then
//             let metadata = Metadata::read(temp_dir.path()).unwrap();
//             assert_eq!(metadata.chain_config, PathBuf::from(CHAIN_CONFIG_FILENAME));
//             assert_eq!(
//                 metadata.state_encoding,
//                 TableEncoding::Json {
//                     filepath: PathBuf::from("state_config.json"),
//                 }
//             );
//         }
//
//         #[test]
//         fn reading_a_snapshot_produces_valid_paths() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//             SnapshotMetadata::write_json(temp_dir.path()).unwrap();
//
//             // when
//             let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();
//
//             // then
//             assert_eq!(
//                 snapshot.chain_config(),
//                 temp_dir.path().join(CHAIN_CONFIG_FILENAME)
//             );
//             assert_eq!(
//                 snapshot.state_encoding(),
//                 &TableEncoding::Json {
//                     filepath: temp_dir.path().join("state_config.json"),
//                 }
//             );
//         }
//
//         #[test]
//         fn filepaths_leading_outside_snapshot_dir_correctly_resolved() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//
//             let snapshot_path = temp_dir.path().join("snapshot");
//             std::fs::create_dir_all(&snapshot_path).unwrap();
//
//             let metadata = Metadata {
//                 chain_config: "../other/chain_config.json".into(),
//                 state_encoding: TableEncoding::Json {
//                     filepath: "../other/state_config.json".into(),
//                 },
//             };
//             metadata.write(&snapshot_path).unwrap();
//
//             // when
//             let snapshot = SnapshotMetadata::read(&snapshot_path).unwrap();
//
//             // then
//             assert_eq!(
//                 snapshot.chain_config(),
//                 snapshot_path.join("../other/chain_config.json")
//             );
//             assert_eq!(
//                 snapshot.state_encoding(),
//                 &TableEncoding::Json {
//                     filepath: snapshot_path.join("../other/state_config.json"),
//                 }
//             );
//         }
//     }
//
//     #[cfg(feature = "parquet")]
//     mod parquet {
//         use super::*;
//
//         #[test]
//         fn new_snapshot_filepaths_relative_to_given_dir() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//
//             // when
//             let snapshot = SnapshotMetadata::write_parquet(
//                 temp_dir.path(),
//                 crate::ZstdCompressionLevel::Max,
//                 10,
//             )
//             .unwrap();
//
//             // then
//             assert_eq!(
//                 snapshot.chain_config(),
//                 temp_dir.path().join(CHAIN_CONFIG_FILENAME)
//             );
//             assert_eq!(
//                 snapshot.state_encoding(),
//                 &TableEncoding::Parquet {
//                     files: vec![
//                         coins: temp_dir.path().join("coins.parquet"),
//                         messages: temp_dir.path().join("messages.parquet"),
//                         contracts: temp_dir.path().join("contracts.parquet"),
//                         contract_state: temp_dir.path().join("contract_state.parquet"),
//                         contract_balance: temp_dir
//                             .path()
//                             .join("contract_balance.parquet"),
//                         block_height: temp_dir.path().join("block_height.parquet"),
//                         da_block_height: temp_dir.path().join("da_block_height.parquet"),
//                     },
//                     compression: crate::ZstdCompressionLevel::Max,
//                     group_size: 10,
//                 }
//             );
//         }
//
//         #[test]
//         fn written_filepaths_dont_have_snapshot_dir_in_path() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//
//             // when
//             SnapshotMetadata::write_parquet(
//                 temp_dir.path(),
//                 crate::ZstdCompressionLevel::Max,
//                 10,
//             )
//             .unwrap();
//
//             // then
//             let metadata = Metadata::read(temp_dir.path()).unwrap();
//             assert_eq!(metadata.chain_config, PathBuf::from(CHAIN_CONFIG_FILENAME));
//             assert_eq!(
//                 metadata.state_encoding,
//                 TableEncoding::Parquet {
//                     filepaths: ParquetFiles {
//                         coins: PathBuf::from("./coins.parquet"),
//                         messages: PathBuf::from("./messages.parquet"),
//                         contracts: PathBuf::from("./contracts.parquet"),
//                         contract_state: PathBuf::from("./contract_state.parquet"),
//                         contract_balance: PathBuf::from("./contract_balance.parquet"),
//                         block_height: PathBuf::from("./block_height.parquet"),
//                         da_block_height: PathBuf::from("./da_block_height.parquet"),
//                     },
//                     compression: crate::ZstdCompressionLevel::Max,
//                     group_size: 10,
//                 }
//             );
//         }
//
//         #[test]
//         fn reading_a_snapshot_produces_valid_paths() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//             SnapshotMetadata::write_parquet(
//                 temp_dir.path(),
//                 crate::ZstdCompressionLevel::Max,
//                 10,
//             )
//             .unwrap();
//
//             // when
//             let snapshot = SnapshotMetadata::read(temp_dir.path()).unwrap();
//
//             // then
//             assert_eq!(
//                 snapshot.chain_config(),
//                 temp_dir.path().join(CHAIN_CONFIG_FILENAME)
//             );
//             assert_eq!(
//                 snapshot.state_encoding(),
//                 &TableEncoding::Parquet {
//                     filepaths: ParquetFiles {
//                         coins: temp_dir.path().join("coins.parquet"),
//                         messages: temp_dir.path().join("messages.parquet"),
//                         contracts: temp_dir.path().join("contracts.parquet"),
//                         contract_state: temp_dir.path().join("contract_state.parquet"),
//                         contract_balance: temp_dir
//                             .path()
//                             .join("contract_balance.parquet"),
//                         block_height: temp_dir.path().join("block_height.parquet"),
//                         da_block_height: temp_dir.path().join("da_block_height.parquet"),
//                     },
//                     compression: crate::ZstdCompressionLevel::Max,
//                     group_size: 10,
//                 }
//             );
//         }
//
//         #[test]
//         fn filepaths_leading_outside_snapshot_dir_correctly_resolved() {
//             // given
//             let temp_dir = tempfile::tempdir().unwrap();
//
//             let snapshot_path = temp_dir.path().join("snapshot");
//             std::fs::create_dir_all(&snapshot_path).unwrap();
//
//             let metadata = Metadata {
//                 chain_config: "../other/chain_config.json".into(),
//                 state_encoding: TableEncoding::Parquet {
//                     filepaths: ParquetFiles {
//                         coins: "../other/coins.parquet".into(),
//                         messages: "../other/messages.parquet".into(),
//                         contracts: "../other/contracts.parquet".into(),
//                         contract_state: "../other/contract_state.parquet".into(),
//                         contract_balance: "../other/contract_balance.parquet".into(),
//                         block_height: "../other/block_height.parquet".into(),
//                         da_block_height: "../other/da_block_height.parquet".into(),
//                     },
//                     compression: crate::ZstdCompressionLevel::Max,
//                     group_size: 10,
//                 },
//             };
//             metadata.write(&snapshot_path).unwrap();
//
//             // when
//             let snapshot = SnapshotMetadata::read(&snapshot_path).unwrap();
//
//             // then
//             assert_eq!(
//                 snapshot.chain_config(),
//                 snapshot_path.join("../other/chain_config.json")
//             );
//             assert_eq!(
//                 snapshot.state_encoding(),
//                 &TableEncoding::Parquet {
//                     filepaths: ParquetFiles {
//                         coins: snapshot_path.join("../other/coins.parquet"),
//                         messages: snapshot_path.join("../other/messages.parquet"),
//                         contracts: snapshot_path.join("../other/contracts.parquet"),
//                         contract_state: snapshot_path
//                             .join("../other/contract_state.parquet"),
//                         contract_balance: snapshot_path
//                             .join("../other/contract_balance.parquet"),
//                         block_height: snapshot_path.join("../other/block_height.parquet"),
//                         da_block_height: snapshot_path
//                             .join("../other/da_block_height.parquet"),
//                     },
//                     compression: crate::ZstdCompressionLevel::Max,
//                     group_size: 10,
//                 }
//             );
//         }
//     }
// }
