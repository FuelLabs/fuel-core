# Full system tests

This directory contains tests that run the full system. There are two tests
currently: `tests/integration-tests.rs` and `tests/runner-tests.rs`.

## Integration tests

The tests require that an IPFS node, a Postgres database, and an Ethereum
RPC client are already running on specific ports. For local testing, these
services can be started using the `docker-compose.yml` file in this
directory. The ports are defined in the `docker-compose.yml` file and in
`src/config.rs`

In addition, the tests require the following:

- `graph-node` must have already been built using `cargo build` and must be
  present at `../target/debug/graph-node`
- `yarn` (v1) must be installed and on the `PATH`

Once these prerequisites are in place, the tests can be run using:

```
cargo test -p graph-tests --test integration_tests -- --nocapture
```

The test harness will clear out the database before each run so that it is
possible to keep the same database running across multiple test runs.
Similarly, test contracts will be deployed to the Ethereum client before
each run unless they are already present from previous runs. By keeping the
services running after the test run, it is possible to inspect the database
and the other services to investigate test failures.

The harness starts `graph-node` by itself; it also prints the command line
that was used to start `graph-node`. That makes it possible to run
`graph-node` again, for example, to query it, when a test has failed. The
log output from `graph-node` can be found in the file
`integration-tests/graph-node.log`.

The `subgraph.yaml` file should not reference contracts by their address;
instead they should reference them with `@<NAME>@`, for example,
`@SimpleContract@`. The test harness replaces these placeholders with the
actual contract addresses before deploying the subgraph.

### Adding/changing test contracts

All test contracts are in the `tests/contracts/src` directory. Compiled
versions in `tests/contracts/out` are checked into git, too. When changes to
the contracts are necessary, the contracts need to be recompiled by running
`foundry build` in `tests/contracts`. For that, the tools from
[Foundry](https://getfoundry.sh/) must be installed locally.

When you change anything about the contract setup, you will also need to
adjust the test data in `CONTRACTS` in `src/contract.rs`. On initial setup,
the tests print the address of the deployed contracts to the console. You
can copy these addresses into the `CONTRACTS` array.

When you add a new contract note that contracts must be called the same as
the file they are stored in: the contract stored in `src/FooContract.sol`
must be declared as `contract FooContract` in the Solidity source.

### Testing different version of Graph CLI

The integration tests project is built as Yarn (v1) Workspace, so all dependencies are installed at once for all tests.

We can still control the version of the Graph CLI installed for each test, by changing the versions of `@graphprotocol/graph-cli` / `@graphprotocol/graph-ts` in `package.json`.

The integration tests runner will always run the binary/executable under `{TEST_DIR}/node_modules/.bin/graph`.
