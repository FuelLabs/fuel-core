
# Contributing to graph-node

Welcome to the Graph Protocol! Thanks a ton for your interest in contributing.

If you run into any problems feel free to create an issue. PRs are much appreciated for simple things. Here's [a list of good first issues](https://github.com/graphprotocol/graph-node/labels/good%20first%20issue). If it's something more complex we'd appreciate having a quick chat in GitHub Issues or Discord.

Join the conversation on our [Discord](https://discord.gg/graphprotocol).

Please follow the [Code of Conduct](https://github.com/graphprotocol/graph-node/blob/master/CODE_OF_CONDUCT.md) for all the communications and at events. Thank you!

## Development flow

Install development helpers:

```sh
cargo install cargo-watch
rustup component add rustfmt
```

Set environment variables:

```sh
# Only required when testing the Diesel/Postgres store
export THEGRAPH_STORE_POSTGRES_DIESEL_URL=<Postgres database URL>
```

- **Note** You can follow Docker Compose instructions in [store/test-store/README.md](./store/test-store/README.md#docker-compose) to easily run a Postgres instance and use `postgresql://graph:graph@127.0.0.1:5432/graph-test` as the Postgres database URL value.

While developing, a useful command to run in the background is this:

```sh
cargo watch                       \
    -x "fmt --all"                 \
    -x check                      \
    -x "test -- --test-threads=1" \
    -x "doc --no-deps"
```

This will watch your source directory and continuously do the following on changes:

1.  Build all packages in the workspace `target/`.
2.  Generate docs for all packages in the workspace in `target/doc/`.
3.  Automatically format all your source files.

### Integrations Tests

The tests can (and should) be run against a sharded store. See [store/test-store/README.md](./store/test-store/README.md) for
detailed instructions about how to run the sharded integrations tests.

## Commit messages and pull requests

We use the following format for commit messages:
`{crate-name}: {Brief description of changes}`, for example: `store: Support 'Or' filters`.

If multiple crates are being changed list them all like this: `core,
graphql: Add event source to store` If all (or most) crates are affected
by the commit, start the message with `all: `.

The body of the message can be terse, with just enough information to
explain what the commit does overall. In a lot of cases, more extensive
explanations of _how_ the commit achieves its goal are better as comments
in the code.

Commits in a pull request should be structured in such a way that each
commit consists of a small logical step towards the overall goal of the
pull request. Your pull request should make it as easy as possible for the
reviewer to follow each change you are making. For example, it is a good
idea to separate simple mechanical changes like renaming a method that
touches many files from logic changes. Your pull request should not be
structured into commits according to how you implemented your feature,
often indicated by commit messages like 'Fix problem' or 'Cleanup'. Flex a
bit, and make the world think that you implemented your feature perfectly,
in small logical steps, in one sitting without ever having to touch up
something you did earlier in the pull request. (In reality, that means
you'll use `git rebase -i` a lot)

Please do not merge master into your branch as you develop your pull
request; instead, rebase your branch on top of the latest master if your
pull request branch is long-lived.

We try to keep the history of the `master` branch linear, and avoid merge
commits. Once your pull request is approved, merge it following these
steps:
```
git checkout master
git pull master
git rebase master my/branch
git push -f
git checkout master
git merge my/branch
git push
```

Allegedly, clicking on the `Rebase and merge` button in the Github UI has
the same effect.
