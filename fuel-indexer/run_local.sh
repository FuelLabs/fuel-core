#!/bin/bash

cd schema
./generate_schemas.sh
mv main.db ..


cd ../indexer
cargo b --release
../../target/release/indexer_service configs/sqlite_config.yaml --manifest tests/test_data/demo_manifest.yaml --local > ./service.log &

cd ../api_server
cargo b --release
../../target/release/api_server --config configs/sqlite_config.yaml
