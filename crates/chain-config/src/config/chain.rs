use core::str::FromStr;
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    fuel_crypto::Hasher,
    fuel_tx::{
        ConsensusParameters,
        GasCosts,
        TxParameters,
    },
    fuel_types::{
        AssetId,
        BlockHeight,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};
use std::path::Path;
#[cfg(feature = "std")]
use std::{
    io::ErrorKind,
    path::PathBuf,
};

use crate::{
    genesis::GenesisCommitment,
    ConsensusConfig,
};

// Fuel Network human-readable part for bech32 encoding
pub const FUEL_BECH32_HRP: &str = "fuel";
pub const LOCAL_TESTNET: &str = "local_testnet";
pub const TESTNET_INITIAL_BALANCE: u64 = 10_000_000;

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
    pub height: BlockHeight,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_name: "local".into(),
            block_gas_limit: TxParameters::DEFAULT.max_gas_per_tx * 10, /* TODO: Pick a sensible default */
            consensus_parameters: ConsensusParameters::default(),
            consensus: ConsensusConfig::default_poa(),
            height: Default::default(),
        }
    }
}

impl ChainConfig {
    pub const BASE_ASSET: AssetId = AssetId::zeroed();

    pub fn load_from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let contents = std::fs::read(path.as_ref().join("chain_parameters.json"))?;
        serde_json::from_slice(&contents).map_err(|e| {
            anyhow::Error::new(e).context(format!(
                "an error occurred while loading the chain parameters file"
            ))
        })
    }

    pub fn local_testnet() -> Self {
        Self {
            chain_name: LOCAL_TESTNET.to_string(),
            ..Default::default()
        }
    }
}

#[cfg(feature = "std")]
impl FromStr for ChainConfig {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            LOCAL_TESTNET => Ok(Self::local_testnet()),
            s => {
                // Attempt to load chain config from path
                let path = PathBuf::from(s.to_string());
                let contents = std::fs::read(path)?;
                serde_json::from_slice(&contents).map_err(|e| {
                    std::io::Error::new(
                        ErrorKind::InvalidData,
                        anyhow::Error::new(e).context(format!(
                            "an error occurred while loading the chain config file {s}"
                        )),
                    )
                })
            }
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
            height,
        } = self;

        // TODO: Hash settlement configuration when it will be available.
        let config_hash = *Hasher::default()
            .chain(chain_name.as_bytes())
            .chain(block_gas_limit.to_be_bytes())
            .chain(consensus_parameters.root()?)
            .chain(consensus.root()?)
            .chain(height.to_bytes())
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
