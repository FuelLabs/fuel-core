mod chain;
mod coin;
mod consensus;
mod contract;
mod message;
mod state;

pub use chain::*;
pub use coin::*;
pub use consensus::*;
pub use contract::*;
pub use message::*;
pub use state::*;

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use std::{
        env::temp_dir,
        fs::write,
        path::PathBuf,
    };

    use rand::{rngs::StdRng, SeedableRng, RngCore};

    use super::chain::ChainConfig;

    #[cfg(feature = "std")]
    #[test]
    fn from_str_loads_from_file() {
        // setup chain config in a temp file
        let tmp_file = tmp_path();
        let disk_config = ChainConfig::local_testnet();
        let json = serde_json::to_string_pretty(&disk_config).unwrap();
        write(tmp_file.clone(), json).unwrap();

        // test loading config from file path string
        let load_config: ChainConfig =
            tmp_file.to_string_lossy().into_owned().parse().unwrap();
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

    #[test]
    fn snapshot_configurable_block_height() {
        let mut rng = StdRng::seed_from_u64(2);
        let config = ChainConfig {
            height: rng.next_u32().into(),
            ..ChainConfig::local_testnet()
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_serialize_block_height_config() {
        let mut rng = StdRng::seed_from_u64(2);
        let config = ChainConfig {
            height: rng.next_u32().into(),
            ..ChainConfig::local_testnet()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[cfg(feature = "std")]
    fn tmp_path() -> PathBuf {
        let mut path = temp_dir();
        path.push(rand::random::<u16>().to_string());
        path
    }
}
