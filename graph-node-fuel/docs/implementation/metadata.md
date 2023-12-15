# Metadata and how it is stored

## Mapping subgraph names to deployments

### `subgraphs.subgraph`

List of all known subgraph names. Maintained in the primary, but there is a background job that periodically copies the table from the primary to all other shards. Those copies are used for queries when the primary is down.

| Column            | Type         | Use                                       |
|-------------------|--------------|-------------------------------------------|
| `id`              | `text!`      | primary key, UUID                         |
| `name`            | `text!`      | user-chosen name                          |
| `current_version` | `text`       | `subgraph_version.id` for current version |
| `pending_version` | `text`       | `subgraph_version.id` for pending version |
| `created_at`      | `numeric!`   | UNIX timestamp                            |
| `vid`             | `int8!`      | unused                                    |
| `block_range`     | `int4range!` | unused                                    |

The `id` is used by the hosted explorer to reference the subgraph.


### `subgraphs.subgraph_version`

Mapping of subgraph names from `subgraph` to IPFS hashes. Maintained in the primary, but there is a background job that periodically copies the table from the primary to all other shards. Those copies are used for queries when the primary is down.

| Column        | Type         | Use                     |
|---------------|--------------|-------------------------|
| `id`          | `text!`      | primary key, UUID       |
| `subgraph`    | `text!`      | `subgraph.id`           |
| `deployment`  | `text!`      | IPFS hash of deployment |
| `created_at`  | `numeric`    | UNIX timestamp          |
| `vid`         | `int8!`      | unused                  |
| `block_range` | `int4range!` | unused                  |


## Managing a deployment

Directory of all deployments. Maintained in the primary, but there is a background job that periodically copies the table from the primary to all other shards. Those copies are used for queries when the primary is down.

### `deployment_schemas`

| Column       | Type           | Use                                          |
|--------------|----------------|----------------------------------------------|
| `id`         | `int4!`        | primary key                                  |
| `subgraph`   | `text!`        | IPFS hash of deployment                      |
| `name`       | `text!`        | name of `sgdNNN` schema                      |
| `version`    | `int4!`        | version of data layout in `sgdNNN`           |
| `shard`      | `text!`        | database shard holding data                  |
| `network`    | `text!`        | network/chain used                           |
| `active`     | `boolean!`     | whether to query this copy of the deployment |
| `created_at` | `timestamptz!` |                                              |

There can be multiple copies of the same deployment, but at most one per shard. The `active` flag indicates which of these copies will be used for queries; `graph-node` makes sure that there is always exactly one for each IPFS hash.

### `subgraph_deployment`

Details about a deployment to track sync progress etc. Maintained in the
shard alongside the deployment's data in `sgdNNN`. The table should only
contain frequently changing data, but for historical reasons contains also
static data.

| Column                               | Type       | Use                                          |
|--------------------------------------|------------|----------------------------------------------|
| `id`                                 | `integer!` | primary key, same as `deployment_schemas.id` |
| `deployment`                         | `text!`    | IPFS hash                                    |
| `failed`                             | `boolean!` |                                              |
| `synced`                             | `boolean!` |                                              |
| `earliest_block_number`              | `integer!` | earliest block for which we have data        |
| `latest_ethereum_block_hash`         | `bytea`    | current subgraph head                        |
| `latest_ethereum_block_number`       | `numeric`  |                                              |
| `entity_count`                       | `numeric!` | total number of entities                     |
| `graft_base`                         | `text`     | IPFS hash of graft base                      |
| `graft_block_hash`                   | `bytea`    | graft block                                  |
| `graft_block_number`                 | `numeric`  |                                              |
| `reorg_count`                        | `integer!` |                                              |
| `current_reorg_depth`                | `integer!` |                                              |
| `max_reorg_depth`                    | `integer!` |                                              |
| `fatal_error`                        | `text`     |                                              |
| `non_fatal_errors`                   | `text[]`   |                                              |
| `health`                             | `health!`  |                                              |
| `last_healthy_ethereum_block_hash`   | `bytea`    |                                              |
| `last_healthy_ethereum_block_number` | `numeric`  |                                              |
| `firehose_cursor`                    | `text`     |                                              |
| `debug_fork`                         | `text`     |                                              |

The columns `reorg_count`, `current_reorg_depth`, and `max_reorg_depth` are
set during indexing. They are used to determine whether a reorg happened
while a query was running, and whether that reorg could have affected the
query.

### `subgraph_manifest`

Details about a deployment that rarely change. Maintained in the
shard alongside the deployment's data in `sgdNNN`.

| Column                  | Type       | Use                                                  |
|-------------------------|------------|------------------------------------------------------|
| `id`                    | `integer!` | primary key, same as `deployment_schemas.id`         |
| `spec_version`          | `text!`    |                                                      |
| `description`           | `text`     |                                                      |
| `repository`            | `text`     |                                                      |
| `schema`                | `text!`    | GraphQL schema                                       |
| `features`              | `text[]!`  |                                                      |
| `graph_node_version_id` | `integer`  |                                                      |
| `use_bytea_prefix`      | `boolean!` |                                                      |
| `start_block_hash`      | `bytea`    | Parent of the smallest start block from the manifest |
| `start_block_number`    | `int4`     |                                                      |
| `on_sync`               | `text`     | Additional behavior when deployment becomes synced   |
| `history_blocks`        | `int4!`    | How many blocks of history to keep                   |

### `subgraph_deployment_assignment`

Tracks which index node is indexing a deployment. Maintained in the primary,
but there is a background job that periodically copies the table from the
primary to all other shards.

| Column  | Type  | Use                                         |
|---------|-------|---------------------------------------------|
| id      | int4! | primary key, ref to `deployment_schemas.id` |
| node_id | text! | name of index node                          |

This table could simply be a column on `deployment_schemas`.

### `dynamic_ethereum_contract_data_source`

Stores the dynamic data sources for all subgraphs (will be turned into a
table that lives in each subgraph's namespace `sgdNNN` soon)

### `subgraph_error`

Stores details about errors that subgraphs encounter during indexing.

### Copying of deployments

The tables `active_copies` in the primary, and `subgraphs.copy_state` and
`subgraphs.copy_table_state` are used to track which deployments need to be
copied and how far copying has progressed to make sure that copying works
correctly across index node restarts.

### Influencing query generation

The table `subgraphs.table_stats` stores which tables for a deployment
should have the 'account-like' optimization turned on.

### `subgraphs.subgraph_features`

Details about features that a deployment uses, Maintained in the primary.

| Column         | Type      | Use         |
|----------------|-----------|-------------|
| `id`           | `text!`   | primary key |
| `spec_version` | `text!`   |             |
| `api_version`  | `text`    |             |
| `features`     | `text[]!` |             |
| `data_sources` | `text[]!` |             |
| `handlers`     | `text[]!` |             |
