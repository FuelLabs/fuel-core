#!/bin/bash

set -e

# This script is used to augment .cargo/config during CI to support publishing to a local registry.
# It patches all workspace dependencies to be sourced from the local registry, since it's impossible
# to dry-run all members because package verification requires every dep to be published.

# Setup local registry configuration
dasel put object -f .cargo/config.toml -p toml -t string "registries.local-registry" index="file:///tmp/local-registry"

# Generate patch definitions for all workspace members to use local registry
for member in $(< Cargo.toml dasel -r toml -w json 'workspace.members' | jq -r ".[]"); do
  echo "$member"
  mkdir -p "$member/.cargo"
  touch "$member/.cargo/config.toml"

  # In order to avoid errors about unused patch definitions, we need to make a specialized patch table for
  # each workspace member.
  for dependency in $(< "$member/Cargo.toml" dasel -r toml -m '.dependencies.-'); do
    TYPE=$(< "$member/Cargo.toml" dasel -r toml -n ".dependencies.$dependency.[@]")
    if [[ $TYPE == 'map' ]]; then
        DEP_VERSION=$(dasel -p toml -f "$member/Cargo.toml" -n "dependencies.$dependency.version")
        DEP_PATH=$(dasel -p toml -f "$member/Cargo.toml" -n "dependencies.$dependency.path")
        # If a dep has both a path and version, that means it needs to be sourced from the local registry
        if [[ $DEP_PATH != "null" && $DEP_VERSION != "null" ]]; then
          DEP_PATH=$(basename "$DEP_PATH")
          # add to patch config if dep is versioned and also has local path (indicating a workspace dep)
          dasel put object -f "$member/.cargo/config.toml" -p toml -t string -t string "patch.crates-io.$dependency" registry="local-registry" path="$DEP_PATH"
        fi
    fi
  done
done
