use bech32::{
    ToBase32,
    Variant::Bech32m,
};
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    fuel_crypto::Hasher,
    fuel_tx::{
        ConsensusParameters,
        Input,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        Bytes32,
    },
    fuel_vm::{
        GasCosts,
        GasCostsValues,
        SecretKey,
    },
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
    FromInto,
};
use std::{
    io::ErrorKind,
    path::PathBuf,
    str::FromStr,
};

use crate::{
    config::{
        coin::CoinConfig,
        state::StateConfig,
    },
    default_consensus_dev_key,
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
    #[serde(default)]
    pub initial_state: Option<StateConfig>,
    pub transaction_parameters: ConsensusParameters,
    #[serde(default)]
    #[serde_as(as = "FromInto<GasCostsValues>")]
    pub gas_costs: GasCosts,
    pub consensus: ConsensusConfig,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_name: "local".into(),
            block_gas_limit: ConsensusParameters::DEFAULT.max_gas_per_tx * 10, /* TODO: Pick a sensible default */
            transaction_parameters: ConsensusParameters::DEFAULT,
            initial_state: None,
            gas_costs: GasCosts::default(),
            consensus: ConsensusConfig::PoA {
                signing_key: Input::owner(&default_consensus_dev_key().public_key()),
            },
        }
    }
}

impl ChainConfig {
    pub const BASE_ASSET: AssetId = AssetId::zeroed();

    pub fn local_testnet() -> Self {
        // endow some preset accounts with an initial balance
        tracing::info!("Initial Accounts");
        let mut rng = StdRng::seed_from_u64(10);
        let initial_coins = (0..5)
            .map(|_| {
                let secret = fuel_core_types::fuel_crypto::SecretKey::random(&mut rng);
                let address = Address::from(*secret.public_key().hash());
                let bech32_data = Bytes32::new(*address).to_base32();
                let bech32_encoding =
                    bech32::encode(FUEL_BECH32_HRP, bech32_data, Bech32m).unwrap();
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x} [bech32: {}]), Balance({})",
                    secret,
                    address,
                    bech32_encoding,
                    TESTNET_INITIAL_BALANCE
                );
                Self::initial_coin(secret, TESTNET_INITIAL_BALANCE, None)
            })
            .collect_vec();

        Self {
            chain_name: LOCAL_TESTNET.to_string(),
            initial_state: Some(StateConfig {
                coins: Some(initial_coins),
                ..StateConfig::default()
            }),
            ..Default::default()
        }
    }

    pub fn initial_coin(
        secret: SecretKey,
        amount: u64,
        utxo_id: Option<UtxoId>,
    ) -> CoinConfig {
        let address = Address::from(*secret.public_key().hash());

        CoinConfig {
            tx_id: utxo_id.as_ref().map(|u| *u.tx_id()),
            output_index: utxo_id.as_ref().map(|u| u.output_index() as u64),
            block_created: None,
            maturity: None,
            owner: address,
            amount,
            asset_id: Default::default(),
        }
    }
}

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
                            "an error occurred while loading the chain config file {}",
                            s
                        )),
                    )
                })
            }
        }
    }
}

impl GenesisCommitment for ChainConfig {
    fn root(&mut self) -> anyhow::Result<MerkleRoot> {
        // TODO: Hash settlement configuration, consensus block production
        let config_hash = *Hasher::default()
            .chain(self.block_gas_limit.to_be_bytes())
            .chain(self.transaction_parameters.root()?)
            .chain(self.chain_name.as_bytes())
            .finalize();

        Ok(config_hash)
    }
}

impl GenesisCommitment for ConsensusParameters {
    fn root(&mut self) -> anyhow::Result<MerkleRoot> {
        // TODO: Define hash algorithm for `ConsensusParameters`
        let bytes = postcard::to_stdvec(&self)?;
        let params_hash = Hasher::default().chain(bytes).finalize();

        Ok(params_hash.into())
    }
}
