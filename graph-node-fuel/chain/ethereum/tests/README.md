Put integration tests for this crate into
`store/test-store/tests/chain/ethereum`. This avoids cyclic dev-dependencies
which make rust-analyzer nearly unusable. Once [this
issue](https://github.com/rust-lang/rust-analyzer/issues/14167) has been
fixed, we can move tests back here
