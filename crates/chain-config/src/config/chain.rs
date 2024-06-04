use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    blockchain::header::StateTransitionBytecodeVersion,
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
pub const BYTECODE_NAME: &str = "state_transition_bytecode.wasm";

#[derive(Clone, derivative::Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[derivative(Debug)]
pub struct ChainConfig {
    pub chain_name: String,
    pub consensus_parameters: ConsensusParameters,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub genesis_state_transition_version: Option<StateTransitionBytecodeVersion>,
    /// Note: The state transition bytecode is stored in a separate file
    /// under the `BYTECODE_NAME` name in serialization form.
    #[serde(skip)]
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
            genesis_state_transition_version: Some(
                fuel_core_types::blockchain::header::LATEST_STATE_TRANSITION_VERSION,
            ),
            // Note: It is invalid bytecode.
            state_transition_bytecode: vec![123; 1024],
            consensus: ConsensusConfig::default_poa(),
        }
    }
}

impl ChainConfig {
    pub const BASE_ASSET: AssetId = AssetId::zeroed();

    #[cfg(feature = "std")]
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        use std::io::Read;
        let path = path.as_ref();
        let mut json = String::new();
        std::fs::File::open(path)?.read_to_string(&mut json)?;
        let mut chain_config: ChainConfig =
            serde_json::from_str(json.as_str()).map_err(|e| {
                anyhow::Error::new(e).context(format!(
                    "an error occurred while loading the chain state file: {:?}",
                    path.to_str()
                ))
            })?;

        let bytecode_path = path.with_file_name(BYTECODE_NAME);
        let bytecode = std::fs::read(bytecode_path).map_err(|e| {
            anyhow::Error::new(e).context(format!(
                "an error occurred while loading the state transition bytecode: {:?}",
                path.to_str()
            ))
        })?;

        chain_config.state_transition_bytecode = bytecode;

        Ok(chain_config)
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

        let bytecode_path = path.as_ref().with_file_name(BYTECODE_NAME);
        let chain_config_file = File::create(path)?;

        serde_json::to_writer_pretty(chain_config_file, self)
            .context("failed to dump chain parameters snapshot to JSON")?;
        std::fs::write(bytecode_path, &self.state_transition_bytecode)
            .context("failed to write state transition bytecode")?;

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
        let chain_config_bytes =
            postcard::to_allocvec(&self).map_err(anyhow::Error::msg)?;
        let config_hash = *Hasher::default()
            .chain(chain_config_bytes.as_slice())
            .chain(self.state_transition_bytecode.as_slice())
            .finalize();

        Ok(config_hash)
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
}
