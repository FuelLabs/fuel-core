use fuel_core::chain_config::ChainConfig;

#[test_case::test_case(
    include_str!("../../deployment/scripts/chainspec/beta_chainspec.json"); 
    "Beta chainconfig"
)]
#[test_case::test_case(
    include_str!("../../deployment/scripts/chainspec/dev_chainspec.json"); 
    "Dev chainconfig"
)]
fn test_deployment_chainconfig(bytes: &str) {
    let chain_config: ChainConfig =
        serde_json::from_str(bytes).expect("Should be able to decode chain config");
    let actual_bytes = serde_json::to_string_pretty(&chain_config)
        .expect("Should be able to encode the chain config");
    assert_eq!(actual_bytes, bytes);
}
