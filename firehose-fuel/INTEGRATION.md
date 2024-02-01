# Chain Integration Document

## Concepts
Blockchain data extraction occurs by a process `Reader`, and a firehose enabled node. We run an instrumented version of a process (usually a node) to sync the chain referred to as `Firehose`.
The 'Firehose' process instruments the blockchain and outputs logs over the standard output pipe, which is subsequently read and processed by the `Reader` process.
The Reader process will read, and stitch together the output of `Firehose` to create rich blockchain data models, which it will subsequently write to
files. The data models in question are [Google Protobuf Structures](https://developers.google.com/protocol-buffers).

#### Data Modeling

Designing the  [Google Protobuf Structures](https://developers.google.com/protocol-buffers) for your given blockchain is one of the most important steps in an integrators journey.
The data structures needs to represent as precisely as possible the on chain data and concepts. By carefully crafting the Protobuf structure, the next steps will be a lot simpler.
The data model need.

As a reference, here is Ethereum's Protobuf Structure:
[https://github.com/streamingfast/firehose-ethereum/blob/develop/proto/sf/ethereum/type/v2/type.proto](https://github.com/streamingfast/firehose-ethereum/blob/develop/proto/sf/ethereum/type/v2/type.proto)

# Running the Demo Chain

We have built an end-to-end template, to start the on-boarding process of new chains. This solution consist of:

*firehose-acme*
As mentioned above, the `Reader` process consumes the data that is extracted and streamed from `Firehose`. In Actuality the Reader
is one process out of multiple ones that creates the _Firehose_. These processes are launched by one application. This application is
chain specific and by convention, we name is "firehose-<chain-name>". Though this application is chain specific, the structure of the application
is standardized and is quite similar from chain to chain. For convenience, we have create a boiler plate app to help you get started.
We named our chain `Acme` this the app is [firehose-acme](https://github.com/streamingfast/firehose-acme)

*Firehose Logs*
Firehose logs consist of an instrumented syncing node. We have created a "dummy-blockchain" chain to simulate a node process syncing that can be found [https://github.com/streamingfast/dummy-blockchain](https://github.com/streamingfast/dummy-blockchain).

## Setting up the dummy chain

Clone the repository:
```bash
git clone https://github.com/streamingfast/dummy-blockchain.git
cd dummy-blockchain
```

Then build the binary:
```bash
go install ./cmd/dummy-blockchain
```

Ensure the build was successful
```bash
./dummy-blockchain --version
```

Take note of the location of the built `dummy-blockchain` binary, you will need to configure `firehose-acme` with it.

## Setting up firehose-acme

Clone the repository:

```bash
git clone git@github.com:streamingfast/firehose-acme.git
cd firehose-acme
```

Configure firehose test setup

```bash
cd devel/standard/
vi standard.yaml
```

modify the flag `reader-node-path: "dummy-blockchain"` to point to the path of your `dummy-blockchain` binary you compiled above

## Starting and testing Firehose

*all subsequent commands are run from the `devel/standard/` directory*

Start `fireacme`
```bash
./start.sh
```

This will launch `fireacme` application. Behind the scenes we are starting 3 sub processes: `reader-node`, `relayer`, `firehose`

*reader-node*

The reader-node is a process that runs and manages the blockchain node Geth. It consumes the blockchain data that is
extracted from our instrumented Geth node. The instrumented Geth node outputs individual block data. The reader-node
process will either write individual block data into separate files called one-block files or merge 100 blocks data
together and write into a file called 100-block file.

This behaviour is configurable with the reader-node-merge-and-store-directly flag. When running the reader-node
process with reader-node-merge-and-store-directly flag enable, we say the reader is running in merged mode”.
When the flag is disabled, we will refer to the reader as running in its normal mode of operation.

In the scenario where the reader-node process stores one-block files. We can run a merger process on the side which
would merge the one-block files into 100-block files. When we are syncing the chain we will run the reader-node process
in merged mode. When we are synced we will run the reader-node in it’s regular mode of operation (storing one-block files)

The one-block files and 100-block files will be store in data-dir/storage/merged-blocks and data-dir/storage/one-blocks respectively.
The naming convention of the file is the number of the first block in the file.

As the instrumented node process outputs blocks, you can see the merged block files in the working dir

```bash
ls -las ./firehose-data/storage/merged-blocks
```

We have also built tools that allow you to introspect block files:

```bash
go install ../../cmd/fireacme && fireacme tools print blocks --store ./firehose-data/storage/merged-blocks 100
```

At this point we have `reader-node` process running as well a `relayer` & `firehose` process. Both of these processes work together to provide the Firehose data stream.
Once the firehose process is running, it will be listening on port 18015. At it’s core the firehose is a gRPC stream. We can list the available gRPC service

```bash
grpcurl -plaintext localhost:18015 list
```

We can start streaming blocks with `sf.firehose.v2.Stream` Service:

```bash
grpcurl -plaintext -d '{"start_block_num": 10}' -import-path ./proto -proto sf/acme/type/v1/type.proto localhost:18015 sf.firehose.v2.Stream.Blocks
```

# Using `firehose-acme` as a template

One of the main reason we provide a `firehose-acme` repository is to act as a template element that integrators can use to bootstrap
creating the required Firehose chain specific code.

We purposely used `Acme` (and also `acme` and `ACME`) throughout this repository so that integrators can simply copy everything and perform
a global search/replace of this word and use their chain name instead.

As well as this, there is a few files that requires a renaming. Would will find below the instructions to properly make the search/replace
as well as the list of files that should be renamed.

## Cloning

First step is to clone again `firehose-acme` this time to a dedicated repository that will be the one of your chain:

```
git clone git@github.com:streamingfast/firehose-acme.git firehose-<chain>
```

> Don't forget to change `<chain>` by the name of your exact chain like `ethereum` so it would became `firehose-ethereum`

Then we are going to remove the `.git` folder to start fresh:

```
cd firehose-<chain>
rm -rf .git
git init
```

While not required, I suggest to create an initial commit so it's easier to revert back if you make a mistake
or delete a wrong file:

```
git add -A .
git commit -m "Initial commit"
```

## Renaming & Modifications

> **Note** For example purposes, we will use Ethereum as the example target chain. Every Ethereum mentions when you run the command should be replaced by an equivalent value for your own chain!

### Renames

Perform a **case-sensitive** search/replace for the following terms, order is important:

- `github.com/streamingfast/firehose-acme` -> `github.com/<owner>/firehose-<chain>`
- `ghcr.io/streamingfast/firehose-acme` -> `ghcr.io/<owner>/firehose-<chain>`
- `owner: streamingfast` -> `owner: <owner>`
- `fireacme` -> `fire<chain_short>` (for the final binary produced)
- `acme` -> `<chain>` (for variable, identifier and other place not meant for display, `camelCase`)
- `Acme` -> `<Chain>` (for title(s) and display of chain's full name, `titleCase`)
- `ACME` -> `<CHAIN>` (for constants)

> **Note** Don't forget to change `<chain>` (and their variants) by the name of your exact chain like `ethereum` so it would become `ethereum`, `Ethereum` and `ETHEREUM` respectively. The `<chain_short>` should be a shorter version if `<chain>` if you find it too long or have a known short version of it. For example, `ethereum` `<chain_short>` is actually `eth` while `NEAR` chain is the same as `<chain>` so `near`. The `<owner>` value needs to be replaced by GitHub organisation/user that is going to host the `firehose-<chain>` repository, for example if `firehose-ethereum` is going to be hosted at `github.com/ethereum-core/firehose-ethereum`, the `<owner>` here would be `ethereum-core`.

#### Using [sd](https://github.com/chmln/sd)

Here the commands to perform the replacement if you have installed (or install) `sd` tool:

```bash
export owner=org # Change me!
export chain=ethereum # Change me!
export chain_short=eth # Change me!
export chain_title=Ethereum # Change me!
export chain_constant=ETHEREUM # Change me!

find . -type f -not -path "./.git/*" -exec sd -f c "github.com/streamingfast/firehose-acme" "github.com/$owner/firehose-$chain" {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c "ghcr.io/streamingfast/firehose-acme" "ghcr.io/$owner/firehose-$chain" {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c "buf.build/streamingfast/firehose-acme" "buf.build/$owner/firehose-$chain" {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c "owner: streamingfast" "owner: $owner" {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c fireacme fire$chain_short {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c acme $owner {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c Acme $chain_title {} \;`
find . -type f -not -path "./.git/*" -exec sd -f c ACME $chain_constant {} \;`

```

> **Warning** Don't forget to chain `owner`, `chain`, `chain_short`, `chain_title` and `change_constant` by respectively your own GitHub organisation/user, chain name and its variation short, title and constant.

### Files

```
git mv ./devel/fireacme ./devel/fireeth
git mv ./cmd/fireacme ./cmd/fireeth
git mv ./pb/sf/acme ./pb/sf/ethereum
git mv ./proto/sf/acme ./proto/sf/ethereum
```

### Re-generate Protobuf

Once you have performed the renamed of all 3 terms above and file renames, you should re-generate the Protobuf code:

```
cd firehose-<chain>
./types/pb/generate.sh
```

> **Note**  You will require `protoc`, `protoc-gen-go` and `protoc-gen-go-grpc`. The former can be installed following https://grpc.io/docs/protoc-installation/, the last two can be installed respectively with `go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.25.0` and `go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.1.0`.

> **Note** If you see the error message `Could not make proto path relative: sf/<chain>/type/v1/type.proto: No such file or directory`, you probably forgot to renamed `proto/sf/acme` to `proto/sf/<chain>`.

### Commit & Compile

It's time to test our previous steps to ensure it compiles and everything is good. At this point, you need to have the repository created on GitHub, so go ahead, create the repository on GitHub and push the first initial commit we had before performing the modifications.

#### Update `types` dependency

A quirks of the current setup is that `types` folder is actually a dedicated Golang module separated from the main module. This creates some small problem when updating the `types` dependency within the main module. First, ensure that `types` compile:

```
cd types
go test ./...
cd ..
```

Everything should be good, ensure you have perform all the renames. Here the steps to do then update the `types` dependency in the core project.

1. `git add -A types`
1. `git commit -m "Re-generated Protobuf types"`
1. `git push`
1. Remove the line starting with `github.com/<owner>/firehose-<chain>/types` from the `go.mod` file
1. `go get github.com/<owner>/firehose-<chain>/types@master`
1. Test everything with `go test ./...`, that should pass now.

Now the main module has its `types` dependency updated with the newly generated Golang Protobuf code.

> **Note** Change `<owner>` and `firehose-<chain>` by correct values where you project is hosted at.

### Node

Doing a Firehose integration means there is an instrumented node that emits Firehose logs (or if not a node directly, definitely a process that reads and emits Firehose logs).

#### [cmd/fireacme/cli/constants.go](cmd/fireacme/cli/constants.go)

- Replace `ChainExecutableName = "dummy-blockchain"` by the `ChainExecutableName = "<binary>"` where `<binary>` is the node's binary name that should be launched.

#### [devel/standard/standard.yaml](devel/standard/standard.yaml)

- Replace `dummy-blockchain` by the node's binary name that should be launched.
- In string `reader-node-arguments: +--firehose-enabled --block-rate=60` replace `--firehose-enabled --block-rate=60` by the required arguments to launch the Firehose instrumented logs and enable Firehose logs. This will be specific to your chain's integration.

### Dockerfile(s) & GitHub Actions

There is two Docker image created by a build of `firehose-acme`. First, a version described as _vanilla_ where only `fireacme` Golang binary is included and another one described as _bundle_ which includes both the `firacme` binary and the chain's binary that `reader-node` launches.

Here the files that needs to be modified for this. The Dockerfile are all built on Ubuntu 20.04 images.

#### [.github/workflows/docker.yaml](.github/workflows/docker.yaml)

- Replace `ghcr.io/streamingfast/dummy-blockchain` by the node Docker image containing the node's binary (ensures it's an Ubuntu/Debian image you are using).
- Replace `dummy-blockchain` by the node's binary name.

#### [Dockerfile](Dockerfile)

- Replace `ghcr.io/streamingfast/dummy-blockchain` by the node Docker image containing the node's binary (ensures it's an Ubuntu/Debian image you are using).
- Replace `dummy-blockchain` by the node's binary name.
- Change `--from=chain /app/dummy-blockchain` to `--from=chain /<path>/<where>/<node>/<binary>` is.

### CHANGELOG

The changelog file `CHANGELOG.md` is used as part of GitHub CI Actions to generate the the correct release notes. It works by matching the first `## <version>` Markdown error an accumulating everything up to the following `## <version>` header (if any). It takes the full text of that and uses it as the changelog notes as well as generating the array of built files.

Major information should be listed in there about any public changes that could affect operators.

### Testing

Once everything is done, normally tests should be all good and everything should compile properly:

```
go test ./...
```

### Commit

If everything is fine at that point, you are ready to commit everything and push

```
git add -A .
git commit -m "Renamed Acme to <Chain>"
git add remote origin <url>
git push
```

## License

[Apache 2.0](LICENSE)
