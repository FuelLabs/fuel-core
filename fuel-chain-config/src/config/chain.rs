use fuel_core_interfaces::common::{
    fuel_tx::ConsensusParameters,
    fuel_types::{
        Address,
        AssetId,
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
use serde_with::skip_serializing_none;

use std::{
    io::ErrorKind,
    path::PathBuf,
    str::FromStr,
};

use super::{
    coin::CoinConfig,
    state::StateConfig,
};

pub const LOCAL_TESTNET: &str = "local_testnet";
pub const TESTNET_INITIAL_BALANCE: u64 = 10_000_000;

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ChainConfig {
    pub chain_name: String,
    pub block_production: BlockProduction,
    pub block_gas_limit: u64,
    #[serde(default)]
    pub initial_state: Option<StateConfig>,
    pub transaction_parameters: ConsensusParameters,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_name: "local".into(),
            block_production: BlockProduction::ProofOfAuthority {
                trigger: fuel_poa_coordinator::Trigger::Instant,
            },
            block_gas_limit: 1_000_000, // TODO: Pick a sensible default
            transaction_parameters: ConsensusParameters::DEFAULT,
            initial_state: None,
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
                let secret = fuel_core_interfaces::common::fuel_crypto::SecretKey::random(
                    &mut rng,
                );
                let address = Address::from(*secret.public_key().hash());
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x}), Balance({})",
                    secret,
                    address,
                    TESTNET_INITIAL_BALANCE
                );
                CoinConfig {
                    tx_id: None,
                    output_index: None,
                    block_created: None,
                    maturity: None,
                    owner: address,
                    amount: TESTNET_INITIAL_BALANCE,
                    asset_id: Default::default(),
                }
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

/// Block production mode and settings
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockProduction {
    /// Proof-of-authority modes
    ProofOfAuthority {
        #[serde(flatten)]
        trigger: fuel_poa_coordinator::Trigger,
    },
    // TODO:
    // RoundRobin,
    // ProofOfStake,
}
