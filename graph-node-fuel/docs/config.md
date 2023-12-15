# Advanced Graph Node configuration

A TOML configuration file can be used to set more complex configurations than those exposed in the
CLI. The location of the file is passed with the `--config` command line switch. When using a
configuration file, it is not possible to use the options `--postgres-url`,
`--postgres-secondary-hosts`, and `--postgres-host-weights`.

The TOML file consists of four sections:

- `[chains]` sets the endpoints to blockchain clients.
- `[store]` describes the available databases.
- `[ingestor]` sets the name of the node responsible for block ingestion.
- `[deployment]` describes how to place newly deployed subgraphs.

Some of these sections support environment variable expansion out of the box,
most notably Postgres connection strings. The official `graph-node` Docker image
includes [`envsubst`](https://github.com/a8m/envsubst) for more complex use
cases.

## Configuring Multiple Databases

For most use cases, a single Postgres database is sufficient to support a
`graph-node` instance. When a `graph-node` instance outgrows a single
Postgres database, it is possible to split the storage of `graph-node`'s
data across multiple Postgres databases. All databases together form the
store of the `graph-node` instance. Each individual database is called a
_shard_.

The `[store]` section must always have a primary shard configured, which
must be called `primary`. Each shard can have additional read replicas that
are used for responding to queries. Only queries are processed by read
replicas. Indexing and block ingestion will always use the main database.

Any number of additional shards, with their own read replicas, can also be
configured. When read replicas are used, query traffic is split between the
main database and the replicas according to their weights. In the example
below, for the primary shard, no queries will be sent to the main database,
and the replicas will receive 50% of the traffic each. In the `vip` shard,
50% of the traffic goes to the main database, and 50% to the replica.

```toml
[store]
[store.primary]
connection = "postgresql://graph:${PGPASSWORD}@primary/graph"
weight = 0
pool_size = 10
[store.primary.replicas.repl1]
connection = "postgresql://graph:${PGPASSWORD}@primary-repl1/graph"
weight = 1
[store.primary.replicas.repl2]
connection = "postgresql://graph:${PGPASSWORD}@primary-repl2/graph"
weight = 1

[store.vip]
connection = "postgresql://graph:${PGPASSWORD}@${VIP_MAIN}/graph"
weight = 1
pool_size = 10
[store.vip.replicas.repl1]
connection = "postgresql://graph:${PGPASSWORD}@${VIP_REPL1}/graph"
weight = 1
```

The `connection` string must be a valid [libpq connection
string](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING). Before
passing the connection string to Postgres, environment variables embedded
in the string are expanded.

### Setting the `pool_size`

Each shard must indicate how many database connections each `graph-node`
instance should keep in its connection pool for that database. For
replicas, the pool size defaults to the pool size of the main database, but
can also be set explicitly. Such a setting replaces the setting from the
main database.

The `pool_size` can either be a number like in the example above, in which
case any `graph-node` instance will use a connection pool of that size, or a set
of rules that uses different sizes for different `graph-node` instances,
keyed off the `node_id` set on the command line. When using rules, the
`pool_size` is set like this:

```toml
pool_size = [
  { node = "index_node_general_.*", size = 20 },
  { node = "index_node_special_.*", size = 30 },
  { node = "query_node_.*", size = 80 }
]
```

Each rule consists of a regular expression `node` and the size that should
be used if the current instance's `node_id` matches that regular
expression. You can use the command `graphman config pools` to check how
many connections each `graph-node` instance will use, and how many database
connections will be opened by all `graph-node` instance. The rules are
checked in the order in which they are written, and the first one that
matches is used. It is an error if no rule matches.

It is highly recommended to run `graphman config pools $all_nodes` every
time the configuration is changed to make sure that the connection pools
are what is expected. Here, `$all_nodes` should be a list of all the node
names that will use this configuration file.

## Configuring Ethereum Providers

The `[chains]` section controls the ethereum providers that `graph-node`
connects to, and where blocks and other metadata for each chain are
stored. The section consists of the name of the node doing block ingestion
(currently not used), and a list of chains. The configuration for a chain
`name` is specified in the section `[chains.<name>]`, and consists of the
`shard` where chain data is stored and a list of providers for that
chain. For each provider, the following information must be given:

- `label`: a label that is used when logging information about that
  provider (not implemented yet)
- `transport`: one of `rpc`, `ws`, and `ipc`. Defaults to `rpc`.
- `url`: the URL for the provider
- `features`: an array of features that the provider supports, either empty
  or any combination of `traces` and `archive`
- `headers`: HTTP headers to be added on every request. Defaults to none.
- `limit`: the maximum number of subgraphs that can use this provider.
  Defaults to unlimited. At least one provider should be unlimited,
  otherwise `graph-node` might not be able to handle all subgraphs. The
  tracking for this is approximate, and a small amount of deviation from
  this value should be expected. The deviation will be less than 10.

The following example configures two chains, `mainnet` and `kovan`, where
blocks for `mainnet` are stored in the `vip` shard and blocks for `kovan`
are stored in the primary shard. The `mainnet` chain can use two different
providers, whereas `kovan` only has one provider.

```toml
[chains]
ingestor = "block_ingestor_node"
[chains.mainnet]
shard = "vip"
provider = [
  { label = "mainnet1", url = "http://..", features = [], headers = { Authorization = "Bearer foo" } },
  { label = "mainnet2", url = "http://..", features = [ "archive", "traces" ] }
]
[chains.kovan]
shard = "primary"
provider = [ { label = "kovan", url = "http://..", features = [] } ]
```

### Controlling the number of subgraphs using a provider

**This feature is experimental and might be removed in a future release**

Each provider can set a limit for the number of subgraphs that can use this
provider. The measurement of the number of subgraphs using a provider is
approximate and can differ from the true number by a small amount
(generally less than 10)

The limit is set through rules that match on the node name. If a node's
name does not match any rule, the corresponding provider will be disabled
for that node.

If the match property is omitted then the provider will be unlimited on every
node.

It is recommended that at least one provider is generally unlimited.
The limit is set in the following way:

```toml
[chains.mainnet]
shard = "vip"
provider = [
  { label = "mainnet-0", url = "http://..", features = [] },
  { label = "mainnet-1", url = "http://..", features = [],
    match = [
      { name = "some_node_.*", limit = 10 },
      { name = "other_node_.*", limit = 0 } ] } ]
```

Nodes named `some_node_.*` will use `mainnet-1` for at most 10 subgraphs,
and `mainnet-0` for everything else, nodes named `other_node_.*` will never
use `mainnet-1` and always `mainnet-0`. Any node whose name does not match
one of these patterns will not be able to use and `mainnet-1`.

## Controlling Deployment

When `graph-node` receives a request to deploy a new subgraph deployment,
it needs to decide in which shard to store the data for the deployment, and
which of any number of nodes connected to the store should index the
deployment. That decision is based on a number of rules defined in the
`[deployment]` section. Deployment rules can match on the subgraph name and
the network that the deployment is indexing.

Rules are evaluated in order, and the first rule that matches determines
where the deployment is placed. The `match` element of a rule can have a
`name`, a [regular expression](https://docs.rs/regex/1.4.2/regex/#syntax)
that is matched against the subgraph name for the deployment, and a
`network` name that is compared to the network that the new deployment
indexes. The `network` name can either be a string, or a list of strings.

The last rule must not have a `match` statement to make sure that there is
always some shard and some indexer that will work on a deployment.

The rule indicates the name of the `shard` where the data for the
deployment should be stored, which defaults to `primary`, and a list of
`indexers`. For the matching rule, one indexer is chosen from the
`indexers` list so that deployments are spread evenly across all the nodes
mentioned in `indexers`. The names for the indexers must be the same names
that are passed with `--node-id` when those index nodes are started.

Instead of a fixed `shard`, it is also possible to use a list of `shards`;
in that case, the system uses the shard from the given list with the fewest
active deployments in it.

```toml
[deployment]
[[deployment.rule]]
match = { name = "(vip|important)/.*" }
shard = "vip"
indexers = [ "index_node_vip_0", "index_node_vip_1" ]
[[deployment.rule]]
match = { network = "kovan" }
# No shard, so we use the default shard called 'primary'
indexers = [ "index_node_kovan_0" ]
[[deployment.rule]]
match = { network = [ "xdai", "poa-core" ] }
indexers = [ "index_node_other_0" ]
[[deployment.rule]]
# There's no 'match', so any subgraph matches
shards = [ "sharda", "shardb" ]
indexers = [
    "index_node_community_0",
    "index_node_community_1",
    "index_node_community_2",
    "index_node_community_3",
    "index_node_community_4",
    "index_node_community_5"
  ]

```

## Query nodes

Nodes can be configured to explicitly be query nodes by including the
following in the configuration file:

```toml
[general]
query = "<regular expression>"
```

Any node whose `--node-id` matches the regular expression will be set up to
only respond to queries. For now, that only means that the node will not
try to connect to any of the configured Ethereum providers.

## Basic Setup

The following file is equivalent to using the `--postgres-url` command line
option:

```toml
[store]
[store.primary]
connection="<.. postgres-url argument ..>"
[deployment]
[[deployment.rule]]
indexers = [ "<.. list of all indexing nodes ..>" ]
```

## Validating configuration files

A configuration file can be checked for validity with the `config check`
command. Running

```shell
graph-node --config $CONFIG_FILE config check
```

will read the configuration file and print information about syntax errors
and some internal inconsistencies, for example, when a shard that is not
declared as a store is used in a deployment rule.

## Simulating deployment placement

Given a configuration file, placement of newly deployed subgraphs can be
simulated with

```shell
graphman --config $CONFIG_FILE config place some/subgraph mainnet
```

The command will not make any changes, but simply print where that subgraph
would be placed. The output will indicate the database shard that will hold
the subgraph's data, and a list of indexing nodes that could be used for
indexing that subgraph. During deployment, `graph-node` chooses the indexing
nodes with the fewest subgraphs currently assigned from that list.
