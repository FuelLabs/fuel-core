#!/usr/bin/env bash

# Builds Solidity contracts for graph-node integration tests.
#
# This script is meant to be callde as a yarn "script", defined in each "package.json"
# file, found on every test subdirectory.
#
# It ensures that all integration tests subdirectories have no pre-built artifacts
# (abis, bin, generated and build directories), and will exctract ABIs and BINs for
# the artifacts built by truffle.

set -euo pipefail

# Cleanup target directories
rm -rf abis build generated

# Compile contracts into a temporary directory
yarn truffle compile

# Move abi to a directory expected by graph-node
mkdir -p abis bin
jq -r '.abi' truffle_output/Contract.json > abis/Contract.abi
