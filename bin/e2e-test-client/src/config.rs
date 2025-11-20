use crate::SYNC_TIMEOUT;
use fuel_core_types::{fuel_tx::ContractId, fuel_vm::SecretKey};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, time::Duration};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SuiteConfig {
    /// The primary endpoint to connect to
    pub endpoint: String,
    /// Max timeout for syncing between wallets
    /// Default is [`SYNC_TIMEOUT`]
    #[serde(with = "humantime_serde")]
    pub wallet_sync_timeout: Duration,
    /// Enable slower but more stressful tests. Should be used in full E2E tests but not in CI.
    pub full_test: bool,
    /// The contract id of the coinbase contract.
    pub coinbase_contract_id: ContractId,
    /// Wallet A must contain pre-existing funds
    pub wallet_a: ClientConfig,
    pub wallet_b: ClientConfig,
}

impl SuiteConfig {
    pub fn sync_timeout(&self) -> Duration {
        self.wallet_sync_timeout
    }
}

impl Default for SuiteConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4000".to_string(),
            wallet_sync_timeout: SYNC_TIMEOUT,
            full_test: false,
            coinbase_contract_id: ContractId::from_str(
                "0x7777777777777777777777777777777777777777777777777777777777777777",
            )
            .unwrap(),
            wallet_a: ClientConfig {
                endpoint: None,
                secret:
                    "0xde97d8624a438121b86a1956544bd72ed68cd69f2c99555b08b1e8c51ffd511c"
                        .parse()
                        .unwrap(),
            },
            wallet_b: ClientConfig {
                endpoint: None,
                secret:
                    "0x37fa81c84ccd547c30c176b118d5cb892bdb113e8e80141f266519422ef9eefd"
                        .parse()
                        .unwrap(),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ClientConfig {
    // overrides the default endpoint for the suite
    pub endpoint: Option<String>,
    // the account to use
    pub secret: SecretKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_snapshot() {
        let config = SuiteConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        insta::assert_snapshot!(serialized);
    }

    #[test]
    fn can_roundtrip_config() {
        let config = SuiteConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: SuiteConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}
