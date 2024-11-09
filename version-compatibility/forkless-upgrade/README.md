# Forkless upgrade tests

This crate tests that state transition functions for all releases of `fuel-core` are backward compatible. 

In addition, we also test that releases are forward-compatible unless we introduce a breaking change in the API.

## Adding new test

We need to add a new backward compatibility test for each new release. To add tests, we need to duplicate tests that are using the latest `fuel-core` and replace usage of the latest crate with a new release.

## Forward compatibility

If the forward compatibility test fails after your changes, it usually means that the change breaks a WASM API, and the network first must upgrade the binary before performing an upgrade of the network.

In the case of breaking API, we need to remove old tests(usually, we need to create a new test per each release) and write a new test(only one) to track new forward compatibility.