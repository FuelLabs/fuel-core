## Pruning deployments

Subgraphs, by default, store a full version history for entities, allowing
consumers to query the subgraph as of any historical block. Pruning is an
operation that deletes entity versions from a deployment older than a
certain block, so it is no longer possible to query the deployment as of
prior blocks. In GraphQL, those are only queries with a constraint `block {
number: <n> } }` or a similar constraint by block hash where `n` is before
the block to which the deployment is pruned. Queries that are run at a
block height greater than that are not affected by pruning, and there is no
difference between running these queries against an unpruned and a pruned
deployment.

Because pruning reduces the amount of data in a deployment, it reduces the
amount of storage needed for that deployment, and is beneficial for both
query performance and indexing speed. Especially compared to the default of
keeping all history for a deployment, it can often reduce the amount of
data for a deployment by a very large amount and speed up queries
considerably. See [caveats](#caveats) below for the downsides.

The block `b` to which a deployment is pruned is controlled by how many
blocks `history_blocks` of history to retain; `b` is calculated internally
using `history_blocks` and the latest block of the deployment when the
prune operation is performed. When pruning finishes, it updates the
`earliest_block` for the deployment. The `earliest_block` can be retrieved
through the `index-node` status API, and `graph-node` will return an error
for any query that tries to time-travel to a point before
`earliest_block`. The value of `history_blocks` must be greater than
`ETHEREUM_REORG_THRESHOLD` to make sure that reverts can never conflict
with pruning.

Pruning is started by running `graphman prune`. That command will perform
an initial prune of the deployment and set the subgraph's `history_blocks`
setting which is used to periodically check whether the deployment has
accumulated more history than that. Whenever the deployment does contain
more history than that, the deployment is automatically repruned. If
ongoing pruning is not desired, pass the `--once` flag to `graphman
prune`. Ongoing pruning can be turned off by setting `history_blocks` to a
very large value with the `--history` flag.

Repruning is performed whenever the deployment has more than
`history_blocks * GRAPH_STORE_HISTORY_SLACK_FACTOR` blocks of history. The
environment variable `GRAPH_STORE_HISTORY_SLACK_FACTOR` therefore controls
how often repruning is performed: with
`GRAPH_STORE_HISTORY_SLACK_FACTOR=1.5` and `history_blocks` set to 10,000,
a reprune will happen every 5,000 blocks. After the initial pruning, a
reprune therefore happens every `history_blocks * (1 -
GRAPH_STORE_HISTORY_SLACK_FACTOR)` blocks. This value should be set high
enough so that repruning occurs relatively infrequently to not cause too
much database work.

Pruning uses two different strategies for how to remove unneeded data:
rebuilding tables and deleting old entity versions. Deleting old entity
versions is straightforward: this strategy deletes rows from the underlying
tables. Rebuilding tables will copy the data that should be kept from the
existing tables into new tables and then replaces the existing tables with
these much smaller tables. Which strategy to use is determined for each
table individually, and governed by the settings for
`GRAPH_STORE_HISTORY_REBUILD_THRESHOLD` and
`GRAPH_STORE_HISTORY_DELETE_THRESHOLD`, both numbers between 0 and 1: if we
estimate that we will remove more than `REBUILD_THRESHOLD` of the table,
the table will be rebuilt. If we estimate that we will remove a fraction
between `REBUILD_THRESHOLD` and `DELETE_THRESHOLD` of the table, unneeded
entity versions will be deleted. If we estimate to remove less than
`DELETE_THRESHOLD`, the table is not changed at all. With both strategies,
operations are broken into batches that should each take
`GRAPH_STORE_BATCH_TARGET_DURATION` seconds to avoid causing very
long-running transactions.

Pruning, in most cases, runs in parallel with indexing and does not block
it. When the rebuild strategy is used, pruning does block indexing while it
copies non-final entities from the existing table to the new table.

The initial prune started by `graphman prune` prints a progress report on
the console. For the ongoing prune runs that are periodically performed,
the following information is logged: a message `Start pruning historical
entities` which includes the earliest and latest block, a message `Analyzed
N tables`, and a message `Finished pruning entities` with details about how
much was deleted or copied and how long that took. Pruning analyzes tables,
if that seems necessary, because its estimates of how much of a table is
likely not needed are based on Postgres statistics.

### Caveats

Pruning is a user-visible operation and does affect some of the things that
can be done with a deployment:

* because it removes history, it restricts how far back time-travel queries
  can be performed. This will only be an issue for entities that keep
  lifetime statistics about some object (e.g., a token) and are used to
  produce time series: after pruning, it is only possible to produce a time
  series that goes back no more than `history_blocks`. It is very
  beneficial though for entities that keep daily or similar statistics
  about some object as it removes data that is not needed once the time
  period is over, and does not affect how far back time series based on
  these objects can be retrieved.
* it restricts how far back a graft can be performed. Because it removes
  history, it becomes impossible to graft more than `history_blocks` before
  the current deployment head.
