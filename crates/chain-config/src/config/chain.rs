use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    fuel_crypto::Hasher,
    fuel_tx::{
        ConsensusParameters,
        GasCosts,
        TxParameters,
    },
    fuel_types::AssetId,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
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
pub const CHAIN_CONFIG_FILENAME: &str = "chain_config.json";

#[serde_as]
// TODO: Remove not consensus/network fields from `ChainConfig` or create a new config only
//  for consensus/network fields.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ChainConfig {
    pub chain_name: String,
    pub block_gas_limit: u64,
    pub consensus_parameters: ConsensusParameters,
    pub consensus: ConsensusConfig,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_name: "local".into(),
            block_gas_limit: TxParameters::DEFAULT.max_gas_per_tx * 10, /* TODO: Pick a sensible default */
            consensus_parameters: ConsensusParameters::default(),
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
        Self::load(snapshot_metadata.chain_config())
    }

    #[cfg(feature = "std")]
    pub fn write(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        use anyhow::Context;

        let state_writer = File::create(path)?;

        serde_json::to_writer_pretty(state_writer, self)
            .context("failed to dump chain parameters snapshot to JSON")?;

        Ok(())
    }

    pub fn local_testnet() -> Self {
        Self {
            chain_name: LOCAL_TESTNET.to_string(),
            ..Default::default()
        }
    }
}

impl GenesisCommitment for ChainConfig {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        // # Dev-note: If `ChainConfig` got a new field, maybe we need to hash it too.
        // Avoid using the `..` in the code below. Use `_` instead if you don't need to hash
        // the field. Explicit fields help to prevent a bug of missing fields in the hash.
        let ChainConfig {
            chain_name,
            block_gas_limit,
            consensus_parameters,
            consensus,
        } = self;

        // TODO: Hash settlement configuration when it will be available.
        let config_hash = *Hasher::default()
            .chain(chain_name.as_bytes())
            .chain(block_gas_limit.to_be_bytes())
            .chain(consensus_parameters.root()?)
            .chain(consensus.root()?)
            .finalize();

        Ok(config_hash)
    }
}

impl GenesisCommitment for ConsensusParameters {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        // TODO: Define hash algorithm for `ConsensusParameters`
        let bytes = postcard::to_allocvec(&self).map_err(anyhow::Error::msg)?;
        let params_hash = Hasher::default().chain(bytes).finalize();

        Ok(params_hash.into())
    }
}

impl GenesisCommitment for GasCosts {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        // TODO: Define hash algorithm for `GasCosts`
        let bytes = postcard::to_allocvec(&self).map_err(anyhow::Error::msg)?;
        let hash = Hasher::default().chain(bytes).finalize();

        Ok(hash.into())
    }
}

impl GenesisCommitment for ConsensusConfig {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        // TODO: Define hash algorithm for `ConsensusConfig`
        let bytes = postcard::to_allocvec(&self).map_err(anyhow::Error::msg)?;
        let hash = Hasher::default().chain(bytes).finalize();

        Ok(hash.into())
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
