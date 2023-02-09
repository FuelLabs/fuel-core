use fuel_core_types::fuel_vm::SecretKey;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SuiteConfig {
    // the primary endpoint to connect to
    pub endpoint: String,
    // wallet a must have pre-existing funds
    pub wallet_a: ClientConfig,
    pub wallet_b: ClientConfig,
}

impl Default for SuiteConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4000".to_string(),
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
}
