#!/bin/bash
# Copyright 2023 Fuel Labs
# Copyright 2021 dfuse Platform Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

SCRIPT_DIR=$(dirname "$0")
ROOT=$(realpath -e "$SCRIPT_DIR/..")
PROTO_FUEL="$ROOT/proto"

function main() {
  checks

  set -e
  cd "$ROOT/pb"
  generate "sf/fuel/type/v1/type.proto"

  echo "generate.sh - `date` - `whoami`" > ./last_generate.txt
  # echo "FuelLabs/firehose-fuel/proto revision: `GIT_DIR=$ROOT/.git git log -n 1 --pretty=format:%h -- proto`" >> ./last_generate.txt
  echo "FuelLabs/firehose-fuel/proto revision: `git log -n 1 --pretty=format:%h -- proto`" >> ./last_generate.txt
}

# usage:
# - generate <protoPath>
# - generate <protoBasePath/> [<file.proto> ...]
function generate() {
    base=""
    if [[ "$#" -gt 1 ]]; then
      base="$1"; shift
    fi

    for file in "$@"; do
      protoc -I$PROTO_FUEL \
        --go_out=. --go_opt=paths=source_relative \
        --go-grpc_out=. --go-grpc_opt=paths=source_relative,require_unimplemented_servers=false \
         $base$file
    done
}

function checks() {
  # The old `protoc-gen-go` did not accept any flags. Just using `protoc-gen-go --version` in this
  # version waits forever. So we pipe some wrong input to make it exit fast. This in the new version
  # which supports `--version` correctly print the version anyway and discard the standard input
  # so it's good with both version.
  result=`printf "" | protoc-gen-go --version 2>&1 | grep -Eo v[0-9\.]+`
  if [[ "$result" == "" ]]; then
    echo "Your version of 'protoc-gen-go' (at `which protoc-gen-go`) is not recent enough."
    echo ""
    echo "To fix your problem, perform those commands:"
    echo ""
    echo "  go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.25.0"
    echo "  go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.1.0"
    echo ""
    echo "If everything is working as expected, the command:"
    echo ""
    echo "  protoc-gen-go --version"
    echo ""
    echo "Should print 'protoc-gen-go v1.25.0' (if it just hangs, you don't have the correct version)"
    echo "Also ensure that your go binary install path is in your PATH environment variable."
    exit 1
  fi
}

main "$@"