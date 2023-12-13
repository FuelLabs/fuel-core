# Contributing to Fuel Core

Thanks for your interest in contributing to Fuel Core! This document outlines some the conventions on building, running, and testing Fuel Core.

Fuel Core has many dependent repositories. If you need any help or mentoring getting started, understanding the codebase, or anything else, please ask on our [Discord](https://discord.gg/xfpK4Pe).

## Code Standards

We use an RFC process to maintain our code standards. They currently live in the RFC repo: <https://github.com/FuelLabs/rfcs/tree/master/text/code-standards>

## Building and setting up a development workspace

Fuel Core is mostly written in Rust, but includes components written in C++ (RocksDB).
We are currently using the latest Rust stable toolchain to build the project.
But for `rustfmt`, we use Rust nightly toolchain because it provides more code style features(you can check [`rustfmt.toml`](.rustfmt.toml)).

### Prerequisites

To build Fuel Core you'll need to at least have the following installed:

-   `git` - version control
-   [`rustup`](https://rustup.rs/) - Rust installer and toolchain manager
-   [`clang`](http://releases.llvm.org/download.html) - Used to build system libraries (required for rocksdb).
-   [`protoc`](https://grpc.io/docs/protoc-installation/) - Used to compile Protocol Buffer files (required by libp2p).

See the [README.md](README.md#system-requirements) for platform specific setup steps.

### Getting the repository

```sh
git clone https://github.com/FuelLabs/fuel-core
```

### Configuring your Rust toolchain

`rustup` is the official toolchain manager for Rust.

We use some additional components such as `clippy` and `rustfmt`(nightly), to install those:

```sh
rustup component add clippy
rustup toolchain install nightly
rustup component add rustfmt --toolchain nightly
```

### Building and testing

Instead of a makefile, Fuel Core uses the `xtask` pattern to manage custom build processes.

You can build Fuel Core:

```sh
cargo xtask build
```

This command will run `cargo build` and also dump the latest schema into `crates/client/assets/schema.sdl` folder.

Linting is done using rustfmt and clippy, which are each separate commands:

```sh
cargo +nightly fmt --all
```

```sh
cargo clippy --all-targets
```

The test suite follows the Rust cargo standards. The GraphQL service will be instantiated by
Tower and will emulate a server/client structure.

Testing is simply done using Cargo:

```sh
cargo test --all-targets
```

#### Build Options

For optimal performance, we recommend using native builds. The generated binary will be optimized for your CPU and may contain specific instructions supported only in your hardware.

To build, run:
`$ RUSTFLAGS="-C target-cpu=native" cargo build --release --bin fuel-core-bin`

The generated binary will be located in `./target/release/fuel-core`

### Build issues

Due to dependencies on external components such as RocksDb, build times can be large without caching.
Using an in-memory (hashmap) based database is supported for testing purposes, so build times can be improved by disabling
default features.

```sh
cargo build -p fuel-core-bin --no-default-features
```

## Contribution flow

This is a rough outline of what a contributor's workflow looks like:

-   Make sure what you want to contribute is already traced as an issue.
    -   We may discuss the problem and solution in the issue.
-   Create a Git branch from where you want to base your work. This is usually master.
-   Write code, add test cases, and commit your work.
-   Run tests and make sure all tests pass.
-   If the PR contains any breaking changes, add the breaking label to your PR.
-   If you are part of the FuelLabs Github org, please open a PR from the repository itself.
-   Otherwise, push your changes to a branch in your fork of the repository and submit a pull request.
    -   Make sure mention the issue, which is created at step 1, in the commit message.
-   Your PR will be reviewed and some changes may be requested.
    -   Once you've made changes, your PR must be re-reviewed and approved.
    -   If the PR becomes out of date, you can use GitHub's 'update branch' button.
    -   If there are conflicts, you can merge and resolve them locally. Then push to your PR branch.
        Any changes to the branch will require a re-review.
-   Our CI system (Github Actions) automatically tests all authorized pull requests.
-   Use Github to merge the PR once approved.

Thanks for your contributions!

### Finding something to work on

For beginners, we have prepared many suitable tasks for you. Checkout our [Help Wanted issues](https://github.com/FuelLabs/fuel-core/issues?q=is%3Aopen+is%3Aissue+label%3A%22help+wanted%22) for a list.

If you are planning something big, for example, relates to multiple components or changes current behaviors, make sure to open an issue to discuss with us before going on.

The Client team actively develops and maintains several dependencies used in Fuel Core, which you may be also interested in:

-   [fuel-types](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-types)
-   [fuel-merkle](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-merkle)
-   [fuel-tx](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-tx)
-   [fuel-asm](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-asm)
-   [fuel-vm](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-vm)

### Linking issues

Pull Requests should be linked to at least one issue in the same repo.

If the pull request resolves the relevant issues, and you want GitHub to close these issues automatically after it merged into the default branch, you can use the syntax (`KEYWORD #ISSUE-NUMBER`) like this:

```md
close #123
```

If the pull request links an issue but does not close it, you can use the keyword `ref` like this:

```md
ref #456
```

Multiple issues should use full syntax for each issue and separate by a comma, like:

```md
close #123, ref #456
```
