# Sharding

When a `graph-node` installation grows beyond what a single Postgres
instance can handle, it is possible to scale the system horizontally by
adding more Postgres instances. This is called _sharding_ and each Postgres
instance is called a _shard_. The resulting `graph-node` system uses all
these Postgres instances together, essentially forming a distributed
database. Sharding relies heavily on the fact that in almost all cases the
traffic for a single subgraph can be handled by a single Postgres instance,
and load can be distributed by storing different subgraphs in different
shards.

In a sharded setup, one shard is special, and is called the _primary_. The
primary is used to store system-wide metadata such as the mapping of
subgraph names to IPFS hashes, a directory of all subgraphs and the shards
in which each is stored, or the list of configured chains. In general,
metadata that rarely changes is stored in the primary whereas metadata that
changes frequently such as the subgraph head pointer is stored in the
shards. The details of which metadata tables are stored where can be found
in [this document](./implementation/metadata.md).

## Setting up

Sharding requires that `graph-node` uses a [configuration file](./config.md)
rather than the older mechanism of configuring `graph-node` entirely with
environment variables. It is configured by adding additional
`[store.<name>]` entries to `graph-node.toml` as described
[here](./config.md#configuring-multiple-databases)

In a sharded setup, shards communicate with each other using the
[`postgres_fdw`](https://www.postgresql.org/docs/current/postgres-fdw.html)
extension. `graph-node` sets up the required foreign servers and foreign
tables to achieve this. It uses the connection information from the
configuration file for that which requires that the `connection` string for
each shard is in the form `postgres://USER:PASSWORD@HOST[:PORT]/DB` since
`graph-node` needs to parse the connection string to extract these
components.

Before setting up sharding, it is important to make sure that the shards can
talk to each other. That requires in particular that firewall rules allow
traffic from each shard to each other shard, and that authentication
configuration like `pg_hba.conf` allows connections from all the other
shards using the target shard's credentials.

When a new shard is added to the configuration file, `graph-node` will
initialize the database schema of that shard during startup. Once the schema
has been initialized, it is possible to manually check inter-shard
connectivity by running `select count(*) from primary_public.chains;` and
`select count(*) from shard_<name>_subgraphs.subgraph` --- the result of
these queries doesn't matter, it only matters that they succeed.

With mutliple shards, `graph-node` will periodically copy some metadata from
the primary to all the other shards. The metadata that gets copied is the
metadata that is needed to repsond to queries as each query needs the
primary to find the shard that stores the subgraph's data. The copies of the
metadata are used when the primary is down to ensure that queries can still
be answered.

## Best practices

Usually, a `graph-node` installation starts out with a single shard. When a
new shard is added, the original shard, which is now called the _primary_,
can still be used in the same way it was used before, and existing subgraphs
and block caches can remain in the primary.

Data can be added to new shards by setting up [deployment
rules](./config.md#controlling-deployment) that send certain subgraphs to
the new shard. It is also possible to store the block cache for new chains
in a new shard by setting the `shard` attribute of the [chain
definition](./config.md#configuring-ethereum-providers)

With shards, there are many possibilities how data can be split between
them. One possible setup is:

- a small primary that mostly stores metadata
- multiple shards for low-traffic subgraphs with a large number of subgraphs
  per shard
- one or a small number of shards for high-traffic subgraphs with a small
  number of subgraphs per shard
- one or more dedicated shards that store only block caches

## Copying between shards

Besides deployment rules for new subgraphs, it is also possible to copy and
move subgraphs between shards. The command `graphman copy create` starts the
process of copying a subgraph from one shard to another. It is possible to
have a copy of the same deployment, identified by an IPFS hash, in multiple
shards, but only one copy can exist in each shard. If a deployment has
multiple copies, exactly one of them is marked as `active` and is the one
that is used to respond to queries. The copies are indexed independently
from each other, according to how they are assigned to index nodes.

By default, `graphman copy create` will copy the data of the source subgraph
up to the point where the copy was initiated and then start indexing the
subgraph independently from its source. When the `--activate` flag is passed
to `graphman copy create`, the copy process will mark the copy as `active`
once copying has finished and the copy has caught up to the chain head. When
the `--replace` flag is passed, the copy process will also mark the source
of the copy as unused, so that the unused deployment reaper built into
`graph-node` will eventually delete it. In the default configuration, the
source will be deleted about 8 hours after the copy has synced to the chain
head.

When a subgraph has multiple copies, copies that are not `active` can be
made eligible for deletion by simply unassigning them. The unused deployment
reaper will eventually delete them.

Copying a deployment can, depending on the size of the deployment, take a
long time. The command `graphman copy stats sgdDEST` can be used to check on
the progress of the copy. Copying also periodically logs progress messages.
After the data has been copied, the copy process has to perform a few
operations that can take a very long time with not much output. In
particular, it has to count all the entities in a subgraph to update the
`entity_count` of the copy.

During copying, `graph-node` creates a namespace in the destination shard
that has the same `sgdNNN` identifier as the deployment in the source shard
and maps all tables from the source into the destination shard. That
namespace in the destination will be automatically deleted when the copy
finishes.

The command `graphman copy list` can be used to list all currently active or
pending copy operations. The number of active copy operations is restricted
to 5 for each source shard/destination shard pair to limit the amount of
load that copying can put on the shards.

## Namespaces

Sharding creates a few namespaces ('schemas') within Postgres which are used
to import data from one shard into another. These namespaces are:

- `primary_public`: maps some important tables from the primary into each shard
- `shard_<name>_subgraphs`: maps some important tables from each shard into
  every other shard

The code that sets up these mappings is in `ForeignServer::map_primary` and
`ForeignServer::map_metadata`
[here](https://github.com/graphprotocol/graph-node/blob/master/store/postgres/src/connection_pool.rs)

The mappings can be rebuilt by running `graphman database remap`.

The split of metadata between the primary and the shards currently poses
some issues for dashboarding data that requires information from both the
primary and a shard. That will be improved in a future release.

## Removing a shard

When a shard is no longer needed, it can be removed from the configuration.
This requires that nothing references that shard anymore. In particular that
means that there is no deployment that is still stored in that shard, and
that no chain is stored in it. If these two conditions are met, removing a
shard is as simple as deleting its declaration from the configuration file.

Removing a shard in this way will leave the foreign tables in
`shard_<name>_subgraphs`, the user mapping and foreign server definition in
all the other shards behind. Those will not hamper the operation of
`graph-node` but can be removed by running the corresponding `DROP` commands
via `psql`.
