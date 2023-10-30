use fuel_core::chain_config::{
    ChainConfig,
    StateConfig,
};

#[test_case::test_case(
    include_str!("../../deployment/scripts/chainspec/beta_chainparams_spec.json"),
    include_str!("../../deployment/scripts/chainspec/beta_chainstate_spec.json");
    "Beta chainconfig"
)]
#[test_case::test_case(
    include_str!("../../deployment/scripts/chainspec/dev_chainparams_spec.json"),
    include_str!("../../deployment/scripts/chainspec/dev_chainstate_spec.json");
    "Dev chainconfig"
)]
fn test_deployment_chainconfig(params_bytes: &str, state_bytes: &str) {
    let chain_params: ChainConfig = serde_json::from_str(params_bytes)
        .expect("Should be able to decode chain config");
    let params_actual_bytes = serde_json::to_string_pretty(&chain_params)
        .expect("Should be able to encode the chain config");

    let chain_state: StateConfig =
        serde_json::from_str(state_bytes).expect("Should be able to decode chain config");
    let state_actual_bytes = serde_json::to_string_pretty(&chain_state)
        .expect("Should be able to encode the chain config");

    assert_eq!(params_actual_bytes, params_bytes);
    assert_eq!(state_actual_bytes, state_bytes);
}
