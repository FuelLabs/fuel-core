use fuel_core::chain_config::{
    ChainConfig,
    StateConfig,
};

#[test_case::test_case(
    "./../deployment/scripts/chainspec/beta",
    include_str!("../../deployment/scripts/chainspec/beta/chain_config.json"),
    include_str!("../../deployment/scripts/chainspec/beta/state_config.json");
    "Beta chainconfig"
)]
#[test_case::test_case(
    "./../deployment/scripts/chainspec/dev",
    include_str!("../../deployment/scripts/chainspec/dev/chain_config.json"),
    include_str!("../../deployment/scripts/chainspec/dev/state_config.json");
    "Dev chainconfig" 
)]
fn test_deployment_chainconfig(dir_path: &str, chain_bytes: &str, state_bytes: &str) {
    let chain_conf = ChainConfig::load_from_directory(dir_path)
        .expect("Should be able to load chain config");
    let chain_actual_bytes = serde_json::to_string_pretty(&chain_conf)
        .expect("Should be able to encode the chain config");

    let state_config = StateConfig::load_from_directory(dir_path)
        .expect("Should be able to load state config");
    let state_actual_bytes = serde_json::to_string_pretty(&state_config)
        .expect("Should be able to encode state config");

    assert_eq!(chain_actual_bytes, chain_bytes);
    assert_eq!(state_actual_bytes, state_bytes);
}
