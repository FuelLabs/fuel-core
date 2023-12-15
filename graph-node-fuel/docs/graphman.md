## Graphman Commands

- [Info](#info)
- [Remove](#remove)
- [Unassign](#unassign)
- [Unused Record](#unused-record)
- [Unused Remove](#unused-remove)
- [Drop](#drop)
- [Chain Check Blocks](#check-blocks)
- [Chain Call Cache Remove](#chain-call-cache-remove)

<a id="info"></a>
# ⌘ Info

### SYNOPSIS

    Prints the details of a deployment

    The deployment can be specified as either a subgraph name, an IPFS hash `Qm..`, or the database
    namespace `sgdNNN`. Since the same IPFS hash can be deployed in multiple shards, it is possible to
    specify the shard by adding `:shard` to the IPFS hash.

    USAGE:
        graphman --config <CONFIG> info [OPTIONS] <DEPLOYMENT>

    ARGS:
        <DEPLOYMENT>
                The deployment (see above)

    OPTIONS:
        -c, --current
                List only current version

        -h, --help
                Print help information

        -p, --pending
                List only pending versions

        -s, --status
                Include status information

        -u, --used
                List only used (current and pending) versions

### DESCRIPTION

The `info` command fetches details for a given deployment from the database.

By default, it shows the following attributes for the deployment:

-   **name**
-   **status** *(`pending` or `current`)*
-   **id** *(the `Qm...` identifier for the deployment's subgraph)*
-   **namespace** *(The database schema which contains that deployment data tables)*
-   **shard**
-   **active** *(If there are multiple entries for the same subgraph, only one of them will be active. That's the one we use for querying)*
-   **chain**
-   **graph node id**

### OPTIONS

If the `--status` option is enabled, extra attributes are also returned:

-   **synced*** *(Whether or not the subgraph has synced all the way to the current chain head)*
-   **health** *(Can be either `healthy`, `unhealthy` (syncing with errors) or `failed`)*
-   **latest indexed block**
-   **current chain head block**

### EXAMPLES

Describe a deployment by its name:

    graphman --config config.toml info subgraph-name

Describe a deployment by its hash:

    graphman --config config.toml info QmfWRZCjT8pri4Amey3e3mb2Bga75Vuh2fPYyNVnmPYL66

Describe a deployment with extra info:

    graphman --config config.toml info QmfWRZCjT8pri4Amey3e3mb2Bga75Vuh2fPYyNVnmPYL66 --status

<a id="remove"></a>
# ⌘ Remove

### SYNOPSIS

    Remove a named subgraph

    USAGE:
        graphman --config <CONFIG> remove <NAME>

    ARGS:
        <NAME>    The name of the subgraph to remove

    OPTIONS:
        -h, --help    Print help information

### DESCRIPTION

Removes the association between a subgraph name and a deployment.

No indexed data is lost as a result of this command.

It is used mostly for stopping query traffic based on the subgraph's name, and to release that name for
another deployment to use.

### EXAMPLES

Remove a named subgraph:

    graphman --config config.toml remove subgraph-name

<a id="unassign"></a>
# ⌘ Unassign

#### SYNOPSIS

    Unassign a deployment

    USAGE:
        graphman --config <CONFIG> unassign <DEPLOYMENT>

    ARGS:
        <DEPLOYMENT>    The deployment (see `help info`)

    OPTIONS:
        -h, --help    Print help information

#### DESCRIPTION

Makes `graph-node` stop indexing a deployment permanently.

No indexed data is lost as a result of this command.

Refer to the [Maintenance Documentation](https://github.com/graphprotocol/graph-node/blob/master/docs/maintenance.md#modifying-assignments) for more details about how Graph Node manages its deployment
assignments.

#### EXAMPLES

Unassign a deployment by its name:

    graphman --config config.toml unassign subgraph-name

Unassign a deployment by its hash:

    graphman --config config.toml unassign QmfWRZCjT8pri4Amey3e3mb2Bga75Vuh2fPYyNVnmPYL66

<a id="unused-record"></a>
# ⌘ Unused Record

### SYNOPSIS

    graphman-unused-record
    Update and record currently unused deployments

    USAGE:
        graphman unused record

    OPTIONS:
        -h, --help    Print help information


### DESCRIPTION

Inspects every shard for unused deployments and registers them in the `unused_deployments` table in the
primary shard.

No indexed data is lost as a result of this command.

This sub-command is used as previous step towards removing all data from unused subgraphs, followed by
`graphman unused remove`.

A deployment is unused if it fulfills all of these criteria:

1.  It is not assigned to a node.
2.  It is either not marked as active or is neither the current or pending version of a subgraph.
3.  It is not the source of a currently running copy operation

### EXAMPLES

To record all unused deployments:

    graphman --config config.toml unused record

<a id="unused-remove"></a>
# ⌘ Unused Remove

### SYNOPSIS

    Remove deployments that were marked as unused with `record`.

    Deployments are removed in descending order of number of entities, i.e., smaller deployments are
    removed before larger ones

    USAGE:
        graphman unused remove [OPTIONS]

    OPTIONS:
        -c, --count <COUNT>
                How many unused deployments to remove (default: all)

        -d, --deployment <DEPLOYMENT>
                Remove a specific deployment

        -h, --help
                Print help information

        -o, --older <OLDER>
                Remove unused deployments that were recorded at least this many minutes ago

### DESCRIPTION

Removes from database all indexed data from deployments previously marked as unused by the `graphman unused
record` command.

This operation is irreversible.

### EXAMPLES

Remove all unused deployments

    graphman --config config.toml unused remove

Remove all unused deployments older than 12 hours (720 minutes)

    graphman --config config.toml unused remove --older 720

Remove a specific unused deployment

    graphman --config config.toml unused remove --deployment QmfWRZCjT8pri4Amey3e3mb2Bga75Vuh2fPYyNVnmPYL66

<a id="drop"></a>
# ⌘ Drop

### SYNOPSIS

    Delete a deployment and all its indexed data

    The deployment can be specified as either a subgraph name, an IPFS hash `Qm..`, or the database
    namespace `sgdNNN`. Since the same IPFS hash can be deployed in multiple shards, it is possible to
    specify the shard by adding `:shard` to the IPFS hash.

    USAGE:
        graphman --config <CONFIG> drop [OPTIONS] <DEPLOYMENT>

    ARGS:
        <DEPLOYMENT>
                The deployment identifier

    OPTIONS:
        -c, --current
                Search only for current versions

        -f, --force
                Skip confirmation prompt

        -h, --help
                Print help information

        -p, --pending
                Search only for pending versions

        -u, --used
                Search only for used (current and pending) versions

### DESCRIPTION

Stops, unassigns and remove all data from deployments matching the search term.

This operation is irreversible.

This command is a combination of other graphman commands applied in sequence:

1. `graphman info <search term>`
2. `graphman unassign <deployment id>`
3. `graphman remove <deployment name>`
4. `graphman unused record`
5. `graphman unused remove <deployment id>`

### EXAMPLES

Stop, unassign and delete all indexed data from a specific deployment by its deployment id

    graphman --config config.toml drop QmfWRZCjT8pri4Amey3e3mb2Bga75Vuh2fPYyNVnmPYL66


Stop, unassign and delete all indexed data from a specific deployment by its subgraph name

    graphman --config config.toml drop author/subgraph-name

<a id="check-blocks"></a>
# ⌘ Check Blocks

### SYNOPSIS

    Compares cached blocks with fresh ones and clears the block cache when they differ

    USAGE:
        graphman --config <config> chain check-blocks <chain-name> <SUBCOMMAND>

    FLAGS:
        -h, --help       Prints help information
        -V, --version    Prints version information

    ARGS:
        <chain-name>    Chain name (must be an existing chain, see 'chain list')

    SUBCOMMANDS:
        by-hash      The number of the target block
        by-number    The hash of the target block
        by-range     A block number range, inclusive on both ends

### DESCRIPTION

The `check-blocks` command compares cached blocks with blocks from a JSON RPC provider and removes any blocks
from the cache that differ from the ones retrieved from the provider.

Sometimes JSON RPC providers send invalid block data to Graph Node. The `graphman chain check-blocks` command
is useful to diagnose the integrity of cached blocks and eventually fix them.

### OPTIONS

Blocks can be selected by different methods. The `check-blocks` command lets you use the block hash, a single
number or a number range to refer to which blocks it should verify:

#### `by-hash`

    graphman --config <config> chain check-blocks <chain-name> by-hash <hash>

#### `by-number`

    graphman --config <config> chain check-blocks <chain-name> by-number <number> [--delete-duplicates]

#### `by-range`

    graphman --config <config> chain check-blocks <chain-name> by-range [-f|--from <block-number>] [-t|--to <block-number>] [--delete-duplicates]

The `by-range` method lets you scan for numeric block ranges and offers the `--from` and `--to` options for
you to define the search bounds. If one of those options is omitted, `graphman` will consider an open bound
and will scan all blocks up to or after that number.

Over time, it can happen that a JSON RPC provider offers different blocks for the same block number. In those
cases, `graphman` will not decide which block hash is the correct one and will abort the operation. Because of
this, the `by-number` and `by-range` methods also provide a `--delete-duplicates` flag, which orients
`graphman` to delete all duplicated blocks for the given number and resume its operation.

### EXAMPLES

Inspect a single Ethereum Mainnet block by hash:

    graphman --config config.toml chain check-blocks mainnet by-hash 0xd56a9f64c7e696cfeb337791a7f4a9e81841aaf4fcad69f9bf2b2e50ad72b972

Inspect a block using its number:

    graphman --config config.toml chain check-blocks mainnet by-number 15626962

Inspect a block range, deleting any duplicated blocks:

    graphman --config config.toml chain check-blocks mainnet by-range --from 15626900 --to 15626962 --delete-duplicates

Inspect all blocks after block `13000000`:

    graphman --config config.toml chain check-blocks mainnet by-range --from 13000000

<a id="chain-call-cache-remove"></a>
# ⌘ Chain Call Cache Remove

### SYNOPSIS

Remove the call cache of the specified chain.

If block numbers are not mentioned in `--from` and `--to`, then all the call cache will be removed.

USAGE:
    graphman chain call-cache <CHAIN_NAME> remove [OPTIONS]

OPTIONS:
    -f, --from <FROM>
            Starting block number

    -h, --help
            Print help information

    -t, --to <TO>
            Ending block number

### DESCRIPTION

Remove the call cache of a specified chain.

### OPTIONS

The `from` and `to` options are used to decide the block range of the call cache that needs to be removed.

#### `from`

The `from` option is used to specify the starting block number of the block range. In the absence of `from` option,
the first block number will be used as the starting block number.

#### `to`

The `to` option is used to specify the ending block number of the block range. In the absence of `to` option,
the last block number will be used as the ending block number.

### EXAMPLES

Remove the call cache for all blocks numbered from 10 to 20:

    graphman --config config.toml chain call-cache ethereum remove --from 10 --to 20

Remove all the call cache of the specified chain:

    graphman --config config.toml chain call-cache ethereum remove

