#!/bin/bash

# Setup local registry configuration
echo ""
echo ""
echo "[registries]"
echo "local-registry = { index = \"file:///tmp/local-registry\" }"
echo ""

# Generate patch definitions for all workspace members to use local registry
echo "[patch.crates-io]"
VERSION=$(< "fuel-core/Cargo.toml" dasel -r toml 'package.version')
for member in $(< Cargo.toml dasel -r toml -w json 'workspace.members' | jq -r ".[]" | xargs -L 1 basename)
do
  # non-publishable packages should have a version of 0.0.0,
  # but overriding their version is harmless since they aren't used by publishable packages.
  echo "$member = { version = \"$VERSION\", registry = \"local-registry\" }"
done
