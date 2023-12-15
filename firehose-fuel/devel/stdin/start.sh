#!/usr/bin/env bash

ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

clean=
firefuel="$ROOT/../firefuel"

main() {
  pushd "$ROOT" &> /dev/null

  while getopts "hc" opt; do
    case $opt in
      h) usage && exit 0;;
      c) clean=true;;
      \?) usage_error "Invalid option: -$OPTARG";;
    esac
  done
  shift $((OPTIND-1))
  [[ $1 = "--" ]] && shift

  set -e

  if [[ $clean == "true" ]]; then
    rm -rf firehose-data &> /dev/null || true
  fi

  chain_data="$ROOT/firehose-data/dummy-blockchain/data"
  if [[ ! -d  "$chain_data" ]]; then
    mkdir -p "$chain_data"
  fi

  if ! command -v dummy-blockchain >/dev/null 2>&1; then
    usage_error "The 'dummy-blockchain' executable must be found within your PATH, install it from source of 'https://github.com/streamingfast/dummy-blockchain'"
  fi

  exec dummy-blockchain --firehose-enabled --block-rate 60 start --store-dir "$chain_data" | $firefuel -c $(basename $ROOT).yaml start "$@"
}

usage_error() {
  message="$1"
  exit_code="$2"

  echo "ERROR: $message"
  echo ""
  usage
  exit ${exit_code:-1}
}

usage() {
  echo "usage: start.sh [-c]"
  echo ""
  echo "Start $(basename $ROOT) environment."
  echo ""
  echo "Options"
  echo "    -c             Clean actual data directory first"
}

main "$@"