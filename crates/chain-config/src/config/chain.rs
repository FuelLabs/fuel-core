use crate::serialization::HexIfHumanReadable;
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    fuel_crypto::Hasher,
    fuel_tx::ConsensusParameters,
    fuel_types::{
        fmt_truncated_hex,
        AssetId,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
#[cfg(feature = "std")]
use std::fs::File;
#[cfg(feature = "std")]
use std::path::Path;

use crate::{
    genesis::GenesisCommitment,
    ConsensusConfig,
};

#[cfg(feature = "std")]
use crate::SnapshotMetadata;

pub const LOCAL_TESTNET: &str = "local_testnet";

#[serde_with::serde_as]
#[derive(Clone, derivative::Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[derivative(Debug)]
pub struct ChainConfig {
    pub chain_name: String,
    pub consensus_parameters: ConsensusParameters,
    #[serde_as(as = "HexIfHumanReadable")]
    #[derivative(Debug(format_with = "fmt_truncated_hex::<16>"))]
    pub state_transition_bytecode: Vec<u8>,
    pub consensus: ConsensusConfig,
}

#[cfg(feature = "test-helpers")]
impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_name: "local".into(),
            consensus_parameters: ConsensusParameters::default(),
            // Note: It is invalid bytecode.
            state_transition_bytecode: vec![],
            consensus: ConsensusConfig::default_poa(),
        }
    }
}

impl ChainConfig {
    pub const BASE_ASSET: AssetId = AssetId::zeroed();

    #[cfg(feature = "std")]
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)?;
        serde_json::from_reader(&file).map_err(|e| {
            anyhow::Error::new(e).context(format!(
                "an error occurred while loading the chain state file: {:?}",
                path.to_str()
            ))
        })
    }

    #[cfg(feature = "std")]
    pub fn from_snapshot_metadata(
        snapshot_metadata: &SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        Self::load(&snapshot_metadata.chain_config)
    }

    #[cfg(feature = "std")]
    pub fn write(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        use anyhow::Context;

        let file = File::create(path)?;

        serde_json::to_writer_pretty(file, self)
            .context("failed to dump chain parameters snapshot to JSON")?;

        Ok(())
    }

    #[cfg(feature = "test-helpers")]
    pub fn local_testnet() -> Self {
        Self {
            chain_name: LOCAL_TESTNET.to_string(),
            ..Default::default()
        }
    }
}

impl GenesisCommitment for ChainConfig {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let bytes = postcard::to_allocvec(&self).map_err(anyhow::Error::msg)?;
        let config_hash = Hasher::default().chain(bytes).finalize();

        Ok(config_hash.into())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use std::env::temp_dir;

    use super::ChainConfig;

    #[cfg(feature = "std")]
    #[test]
    fn can_roundtrip_write_and_read() {
        let tmp_dir = temp_dir();
        let file = tmp_dir.join("config.json");

        let disk_config = ChainConfig::local_testnet();
        disk_config.write(&file).unwrap();

        let load_config = ChainConfig::load(&file).unwrap();

        assert_eq!(disk_config, load_config);
    }

    #[test]
    fn snapshot_local_testnet_config() {
        let config = ChainConfig::local_testnet();
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_serialize_local_testnet_config() {
        let config = ChainConfig::local_testnet();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }
}
