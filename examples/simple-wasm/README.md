# WASM indexer

A simple example of a wasm custom indexer

## building

cargo b --release

## Running as a standalone

cargo r --bin index-runner -- --wasm target/wasm32-wasi/release/simple_wasm.wasm
