# Common maintenance tasks

This document explains how to perform common maintenance tasks using
`graphman`. The `graphman` command is included in the official containers,
and you can `docker exec` into your `graph-node` container to run it. It
requires a [configuration
file](https://github.com/graphprotocol/graph-node/blob/master/docs/config.md). If
you are not using one already, [these
instructions](https://github.com/graphprotocol/graph-node/blob/master/docs/config.md#basic-setup)
show how to create a minimal configuration file that works for `graphman`.

The command pays attention to the `GRAPH_LOG` environment variable, and
will print normal `graph-node` logs on stdout. You can turn them off by
doing `unset GRAPH_LOG` before running `graphman`.

A simple way to check that `graphman` is set up correctly is to run
`graphman info some/subgraph`. If that subgraph exists, the command will
print basic information about it, like the namespace in Postgres that
contains the data for the underlying deployment.

## Removing unused deployments

When a new version of a subgraph is deployed, the new deployment displaces
the old one when it finishes syncing. At that point, the system will not
use the old deployment anymore, but its data is still in the database.

These unused deployments can be removed by running `graphman unused record`
which compiles a list of unused deployments. That list can then be
inspected with `graphman unused list -e`. The data for these unused
deployments can then be removed with `graphman unused remove` which will
only remove the deployments that have previously marked for removal with
`record`.

## Removing a subgraph

The command `graphman remove some/subgraph` will remove the mapping from
the given name to the underlying deployment. If no other subgraph name uses
that deployment, it becomes eligible for removal, and the steps for
removing unused deployments will delete its data.

## Modifying assignments

Each deployment is assigned to a specific `graph-node` instance for
indexing. It is possible to change the `graph-node` instance that indexes a
given subgraph with `graphman reassign`. To permanently stop indexing it,
use `graphman unassign`. Unfortunately, `graphman` does not currently allow
creating an assignment for an unassigned deployment; it is possible to
assign a deployment to a node that does not exist, which will also stop
indexing it, for example by assigning it to a node `paused_<real node
name>`. Indexing can then be resumed by reassigning the deployment to an
existing node.
