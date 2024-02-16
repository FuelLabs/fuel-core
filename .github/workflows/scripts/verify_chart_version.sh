#!/usr/bin/env bash
set -e

err() {
    echo -e "\e[31m\e[1merror:\e[0m $@" 1>&2;
}

status() {
    WIDTH=12
    printf "\e[32m\e[1m%${WIDTH}s\e[0m %s\n" "$1" "$2"
}

# Function to check file existence
file_exists() {
    if [ ! -f "$1" ]; then
        err "File $1 not found."
        exit 1
    fi
}

# Install dasel
curl -sSLf "https://github.com/TomWright/dasel/releases/download/v1.24.3/dasel_linux_amd64" -L -o dasel
chmod +x dasel
mv ./dasel /usr/local/bin/dasel

# Check existence of required files
file_exists "deployment/charts/Chart.yaml"
file_exists "Cargo.toml"

# Check appVersion with fuel-core
HELM_APP_VERSION=$(cat deployment/charts/Chart.yaml | dasel -r yaml 'appVersion')
FUEL_CORE_VERSION=$(cat Cargo.toml | dasel -r toml 'workspace.package.version')

# Comparison of versions
if [ "$HELM_APP_VERSION" != "$FUEL_CORE_VERSION" ]; then
    err "fuel-core version $FUEL_CORE_VERSION does not match helm app version $HELM_APP_VERSION"
    exit 1
else
    status "fuel-core version matches helm chart app version $HELM_APP_VERSION"
fi
