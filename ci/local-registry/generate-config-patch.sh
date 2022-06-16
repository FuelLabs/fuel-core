#!/bin/bash
# This script is used to augment .cargo/config during CI to support publishing to a local registry
# We use patches in .cargo/config to avoid using --allow-dirty when publishing, since we need to
# modify just the workspace members to use our local registry instead of crates-io.

# Setup local registry configuration
dasel put object -f .cargo/config.toml -p toml -t string "registries.local-registry" index="file:///tmp/local-registry"

# Generate patch definitions for all workspace members to use local registry
for member in $(< Cargo.toml dasel -r toml -w json 'workspace.members' | jq -r ".[]"); do
  mkdir "$member/.cargo"
  touch "$member/.cargo/config.toml"

  PACKAGE=$(< "$member/Cargo.toml" dasel -r toml 'package.name')
  VERSION=$(< "$member/Cargo.toml" dasel -r toml 'package.version')

  for dependency in $(< "$member/Cargo.toml" dasel -r toml -m '.dependencies.-'); do
    PATH=$(< "fuel-core/Cargo.toml" dasel -r toml -n ".dependencies.$dependency.path")
    DEP_VERSION=$(< "fuel-core/Cargo.toml" dasel -r toml -n ".dependencies.$dependency.version")
    if [[ $PATH_TYPE != 'null' && $DEP_VERSION != 'null' ]]; then
      # add to patch config if dep is versioned and also has local path (indicating a workspace dep)
      dasel put object -f "$member/.cargo/config.toml" -p toml -t string -t string "patch.crates-io.$dependency" registry="local-registry" path="$PATH"
    fi
  done

  # for any packages that are publishable
  if ! grep -q ^"publish = false" "$member/Cargo.toml"; then
    echo "$PACKAGE = { path = \"$member\", version = \"$VERSION\", registry = \"local-registry\" }";
  fi
done
