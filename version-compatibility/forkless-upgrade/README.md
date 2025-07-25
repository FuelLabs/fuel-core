# Forkless upgrade tests

This crate tests that state transition functions for all releases of `fuel-core` are backward compatible. 

In addition, we also test that releases are forward-compatible unless we introduce a breaking change in the API.

## Adding new test

We need to add a new backward compatibility test for each new release. To add tests, we need to duplicate tests that are using the latest `fuel-core` and replace usage of the latest crate with a new release.

## Forward compatibility

If the forward compatibility test fails after your changes, it usually means that the change breaks a WASM API, and the network first must upgrade the binary before performing an upgrade of the network.

In the case of breaking API, we need to remove old tests(usually, we need to create a new test per each release) and write a new test(only one) to track new forward compatibility.

## Updating Forward Compatibility Test

If at any point the state transition function becomes forward incompatible, we need to update 
`latest_state_transition_function_is_forward_compatible_with_v44_binary` to use the latest version of `fuel-core`.

To update the test, we need to:
- Add a new `chain-configurations` entry for the new version
- Copy over the contents of the previous version. i.e. if we are updating from `v36` to `v44`, we should create a new 
`v44` directory and copy over the contents of `v36` to `v44`.
- Update `latest_state_transition_function_is_forward_compatible_with_v44_binary` to use the new `chain-configurations/` directory
    - Create a new const for the configuration path, e.g. `pub const V44_TESTNET_SNAPSHOT: &str = "./chain-configurations/v44";`
    - Update the test to use the new const
    - Update the STF version to be native:
        - "genesis_state_transition_version" in `chain_config.json`
        - "state_transition_version" for the `latest_block` in `state_config.json`
        - Bump the versions in the test asserts. i.e. if the version in the configs is `28`, then the asserts will be `29` and `30` respectively.
