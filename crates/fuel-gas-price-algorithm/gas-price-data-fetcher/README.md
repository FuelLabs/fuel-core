# Gas Price Analysis Data Fetcher

Binary allowing retrieveing the L1 blob and L2 block data needed by the gas price simulation binary. 
It requires being able to connect to the block commmitter, and either being able to connect to a sentry node or access to the database of a mainnet synched node.

## Usage

```
cargo run -- --block-committer-endpoint ${BLOCK_COMMITTER_URL} --block-range 0 1000 --db-path ${FUEL_MAINNET_DB_PATH}
``` 
