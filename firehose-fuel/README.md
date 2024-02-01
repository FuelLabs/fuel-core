# Firehose Starter for new blockchain integrations
[![reference](https://img.shields.io/badge/godoc-reference-5272B4.svg?style=flat-square)](https://pkg.go.dev/github.com/streamingfast/firehose-acme)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

What's [ACME](https://en.wikipedia.org/wiki/Acme_Corporation)?

# Usage

See [documentation on the Firehose docs website](https://firehose.streamingfast.io/integrate-new-chains/firehose-starter).

## Release

Use https://github.com/streamingfast/sfreleaser to perform a new release. You can install from source https://github.com/streamingfast/sfreleaser/releases downloading the binary.

However if for now suggest to install from source (the tool `sfreleaser` is still getting fixes/features):

```bash
go install github.com/streamingfast/sfreleaser@latest
```

It will ask you questions as well as driving all the required commands, performing the necessary operation automatically. The release is pushed in draft mode by default, so you can test check the whole flow before publishing (See configuration file [.sfreleaser](./.sfreleaser) for some extra details).

You will need to have for releases:
- [Docker](https://docs.docker.com/get-docker/) installed and running
- [GitHub CLI tool](https://cli.github.com/) installed and authenticated with GitHub

The `sfreleaser` binary checks that those tools exist before doing any work.

## Contributing

**Issues and PR in this repo related strictly to the Firehose on Dummy Blockchain.**

Report any protocol-specific issues in their
[respective repositories](https://github.com/streamingfast/streamingfast#protocols)

**Please first refer to the general
[StreamingFast contribution guide](https://github.com/streamingfast/streamingfast/blob/master/CONTRIBUTING.md)**,
if you wish to contribute to this code base.

This codebase uses unit tests extensively, please write and run tests.

## License

[Apache 2.0](LICENSE)
