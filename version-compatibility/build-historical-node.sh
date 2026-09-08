#!/usr/bin/env bash
# Install the historical node independently of the compatibility workspace.
set -euo pipefail

compatibility_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
install_root="$compatibility_dir/target/historical/v0.44.0"

check_version() {
  local binary="$1" version
  if [[ ! -x "$binary" ]]; then
    echo "Historical node is not executable: $binary" >&2
    exit 1
  fi
  if ! version=$("$binary" --version); then
    echo "Failed to read historical node version: $binary" >&2
    exit 1
  fi
  if [[ "$version" != "fuel-core 0.44.0" ]]; then
    echo "Expected fuel-core 0.44.0, got '$version' from $binary" >&2
    exit 1
  fi
  echo "Historical node ready: $binary"
}

if [[ ${FUEL_CORE_V44_BIN+x} ]]; then
  check_version "$FUEL_CORE_V44_BIN"
  exit 0
fi

cargo_command=(cargo)
rustc_command=(rustc)
if [[ -n "${FUEL_CORE_V44_TOOLCHAIN:-}" ]]; then
  cargo_command+=("+$FUEL_CORE_V44_TOOLCHAIN")
  rustc_command+=("+$FUEL_CORE_V44_TOOLCHAIN")
fi
host=$("${rustc_command[@]}" -vV | sed -n 's/^host: //p')
if [[ -z "$host" ]]; then
  echo "Could not determine the historical Rust toolchain's native target" >&2
  exit 1
fi

# Old sources must not inherit the current workspace's warning-as-error policy.
# Keep build artifacts outside the installation root, reusable across installs.
RUSTFLAGS= CARGO_ENCODED_RUSTFLAGS= \
  CARGO_TARGET_DIR="$compatibility_dir/target/historical/build/v0.44.0" \
  "${cargo_command[@]}" install fuel-core-bin --version '=0.44.0' --locked \
  --features parquet,p2p --target "$host" --root "$install_root"

check_version "$install_root/bin/fuel-core"
