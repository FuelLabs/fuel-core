# Environment Variables

**Warning**: the names of some of these environment variables will be changed at
some point in the near future.

This page lists the environment variables used by `graph-node` and what effect
they have. Some environment variables can be used instead of command line flags.
Those are not listed here, please consult `graph-node --help` for details on
those.

## JSON-RPC configuration for EVM chains

- `ETHEREUM_REORG_THRESHOLD`: Maximum expected reorg size, if a larger reorg
  happens, subgraphs might process inconsistent data. Defaults to 250.
- `ETHEREUM_POLLING_INTERVAL`: how often to poll Ethereum for new blocks (in ms,
  defaults to 500ms)
- `GRAPH_ETHEREUM_TARGET_TRIGGERS_PER_BLOCK_RANGE`: The ideal amount of triggers
  to be processed in a batch. If this is too small it may cause too many requests
  to the ethereum node, if it is too large it may cause unreasonably expensive
  calls to the ethereum node and excessive memory usage (defaults to 100).
- `ETHEREUM_TRACE_STREAM_STEP_SIZE`: `graph-node` queries traces for a given
  block range when a subgraph defines call handlers or block handlers with a
  call filter. The value of this variable controls the number of blocks to scan
  in a single RPC request for traces from the Ethereum node. Defaults to 50.
- `DISABLE_BLOCK_INGESTOR`: set to `true` to disable block ingestion. Leave
  unset or set to `false` to leave block ingestion enabled.
- `ETHEREUM_BLOCK_BATCH_SIZE`: number of Ethereum blocks to request in parallel.
  Also limits other parallel requests such as trace_filter. Defaults to 10.
- `GRAPH_ETHEREUM_MAX_BLOCK_RANGE_SIZE`: Maximum number of blocks to scan for
  triggers in each request (defaults to 1000).
- `GRAPH_ETHEREUM_MAX_EVENT_ONLY_RANGE`: Maximum range size for `eth.getLogs`
  requests that don't filter on contract address, only event signature (defaults to 500).
- `GRAPH_ETHEREUM_JSON_RPC_TIMEOUT`: Timeout for Ethereum JSON-RPC requests.
- `GRAPH_ETHEREUM_REQUEST_RETRIES`: Number of times to retry JSON-RPC requests
  made against Ethereum. This is used for requests that will not fail the
  subgraph if the limit is reached, but will simply restart the syncing step,
  so it can be low. This limit guards against scenarios such as requesting a
  block hash that has been reorged. Defaults to 10.
- `GRAPH_ETHEREUM_BLOCK_INGESTOR_MAX_CONCURRENT_JSON_RPC_CALLS_FOR_TXN_RECEIPTS`:
  The maximum number of concurrent requests made against Ethereum for
  requesting transaction receipts during block ingestion.
  Defaults to 1,000.
- `GRAPH_ETHEREUM_FETCH_TXN_RECEIPTS_IN_BATCHES`: Set to `true` to
  disable fetching receipts from the Ethereum node concurrently during
  block ingestion. This will use fewer, batched requests. This is always set to `true`
  on MacOS to avoid DNS issues.
- `GRAPH_ETHEREUM_CLEANUP_BLOCKS` : Set to `true` to clean up unneeded
  blocks from the cache in the database. When this is `false` or unset (the
  default), blocks will never be removed from the block cache. This setting
  should only be used during development to reduce the size of the
  database. In production environments, it will cause multiple downloads of
  the same blocks and therefore slow the system down. This setting can not
  be used if the store uses more than one shard.
- `GRAPH_ETHEREUM_GENESIS_BLOCK_NUMBER`: Specify genesis block number. If the flag
  is not set, the default value will be `0`.

## Running mapping handlers

- `GRAPH_MAPPING_HANDLER_TIMEOUT`: amount of time a mapping handler is allowed to
  take (in seconds, default is unlimited)
- `GRAPH_ENTITY_CACHE_SIZE`: Size of the entity cache, in kilobytes. Defaults to 10000 which is 10MB.
- `GRAPH_MAX_API_VERSION`: Maximum `apiVersion` supported, if a developer tries to create a subgraph
  with a higher `apiVersion` than this in their mappings, they'll receive an error. Defaults to `0.0.7`.
- `GRAPH_MAX_SPEC_VERSION`: Maximum `specVersion` supported. if a developer tries to create a subgraph
  with a higher `apiVersion` than this, they'll receive an error. Defaults to `0.0.5`.
- `GRAPH_RUNTIME_MAX_STACK_SIZE`: Maximum stack size for the WASM runtime, if exceeded the execution
  stops and an error is thrown. Defaults to 512KiB.

## IPFS

- `GRAPH_IPFS_TIMEOUT`: timeout for IPFS, which includes requests for manifest files
  and from mappings (in seconds, default is 60).
- `GRAPH_MAX_IPFS_FILE_BYTES`: maximum size for a file that can be retrieved by an `ipfs cat` call.
  This affects both subgraph definition files and `file/ipfs` data sources. In bytes, default is 25 MiB.
- `GRAPH_MAX_IPFS_MAP_FILE_SIZE`: maximum size of files that can be processed
  with `ipfs.map`. When a file is processed through `ipfs.map`, the entities
  generated from that are kept in memory until the entire file is done
  processing. This setting therefore limits how much memory a call to `ipfs.map`
  may use (in bytes, defaults to 256MB).
- `GRAPH_MAX_IPFS_CACHE_SIZE`: maximum number of files cached (defaults to 50).
- `GRAPH_MAX_IPFS_CACHE_FILE_SIZE`: maximum size of each cached file (in bytes, defaults to 1MiB).
- `GRAPH_IPFS_REQUEST_LIMIT`: Limits the number of requests per second to IPFS for file data sources.
   Defaults to 100.

## GraphQL

- `GRAPH_GRAPHQL_QUERY_TIMEOUT`: maximum execution time for a graphql query, in
  seconds. Default is unlimited.
- `GRAPH_GRAPHQL_MAX_COMPLEXITY`: maximum complexity for a graphql query. See
  [here](https://developer.github.com/v4/guides/resource-limitations) for what
  that means. Default is unlimited. Typical introspection queries have a
  complexity of just over 1 million, so setting a value below that may interfere
  with introspection done by graphql clients.
- `GRAPH_GRAPHQL_MAX_DEPTH`: maximum depth of a graphql query. Default (and
  maximum) is 255.
- `GRAPH_GRAPHQL_MAX_FIRST`: maximum value that can be used for the `first`
  argument in GraphQL queries. If not provided, `first` defaults to 100. The
  default value for `GRAPH_GRAPHQL_MAX_FIRST` is 1000.
- `GRAPH_GRAPHQL_MAX_SKIP`: maximum value that can be used for the `skip`
  argument in GraphQL queries. The default value for
  `GRAPH_GRAPHQL_MAX_SKIP` is unlimited.
- `GRAPH_GRAPHQL_WARN_RESULT_SIZE` and `GRAPH_GRAPHQL_ERROR_RESULT_SIZE`:
  if a GraphQL result is larger than these sizes in bytes, log a warning
  respectively abort query execution and return an error. The size of the
  result is checked while the response is being constructed, so that
  execution does not take more memory than what is configured. The default
  value for both is unlimited.
- `GRAPH_GRAPHQL_MAX_OPERATIONS_PER_CONNECTION`: maximum number of GraphQL
  operations per WebSocket connection. Any operation created after the limit
  will return an error to the client. Default: 1000.
- `GRAPH_GRAPHQL_HTTP_PORT` : Port for the GraphQL HTTP server
- `GRAPH_GRAPHQL_WS_PORT` : Port for the GraphQL WebSocket server
- `GRAPH_SQL_STATEMENT_TIMEOUT`: the maximum number of seconds an
  individual SQL query is allowed to take during GraphQL
  execution. Default: unlimited
- `GRAPH_DISABLE_SUBSCRIPTION_NOTIFICATIONS`: disables the internal
  mechanism that is used to trigger updates on GraphQL subscriptions. When
  this variable is set to any value, `graph-node` will still accept GraphQL
  subscriptions, but they won't receive any updates.
- `ENABLE_GRAPHQL_VALIDATIONS`: enables GraphQL validations, based on the GraphQL specification.
  This will validate and ensure every query executes follows the execution
  rules. Default: `false`
- `SILENT_GRAPHQL_VALIDATIONS`: If `ENABLE_GRAPHQL_VALIDATIONS` is enabled, you are also able to just
  silently print the GraphQL validation errors, without failing the actual query. Note: queries
  might still fail as part of the later stage validations running, during
  GraphQL engine execution. Default: `true`
- `GRAPH_GRAPHQL_DISABLE_BOOL_FILTERS`: disables the ability to use AND/OR
  filters. This is useful if we want to disable filters because of
  performance reasons.
- `GRAPH_GRAPHQL_DISABLE_CHILD_SORTING`: disables the ability to use child-based
  sorting. This is useful if we want to disable child-based sorting because of
  performance reasons.
- `GRAPH_GRAPHQL_TRACE_TOKEN`: the token to use to enable query tracing for
  a GraphQL request. If this is set, requests that have a header
  `X-GraphTraceQuery` set to this value will include a trace of the SQL
  queries that were run. Defaults to the empty string which disables
  tracing.

### GraphQL caching

- `GRAPH_CACHED_SUBGRAPH_IDS`: when set to `*`, cache all subgraphs (default behavior). Otherwise, a comma-separated list of subgraphs for which to cache queries.
- `GRAPH_QUERY_CACHE_BLOCKS`: How many recent blocks per network should be kept in the query cache. This should be kept small since the lookup time and the cache memory usage are proportional to this value. Set to 0 to disable the cache. Defaults to 1.
- `GRAPH_QUERY_CACHE_MAX_MEM`: Maximum total memory to be used by the query cache, in MB. The total amount of memory used for caching will be twice this value - once for recent blocks, divided evenly among the `GRAPH_QUERY_CACHE_BLOCKS`, and once for frequent queries against older blocks. The default is plenty for most loads, particularly if `GRAPH_QUERY_CACHE_BLOCKS` is kept small. Defaults to 1000, which corresponds to 1GB.
- `GRAPH_QUERY_CACHE_STALE_PERIOD`: Number of queries after which a cache entry can be considered stale. Defaults to 100.

## Miscellaneous

- `GRAPH_NODE_ID`: sets the node ID, allowing to run multiple Graph Nodes
  in parallel and deploy to specific nodes; each ID must be unique among the set
  of nodes. A single node should have the same value between consecutive restarts.
  Subgraphs get assigned to node IDs and are not reassigned to other nodes automatically.
- `GRAPH_NODE_ID_USE_LITERAL_VALUE`: (Docker only) Use the literal `node_id`
  provided to the docker start script instead of replacing hyphens (-) in names
  with underscores (\_). Changing this for an existing `graph-node`
  installation requires also changing the assigned node IDs in the
  `subgraphs.subgraph_deployment_assignment` table in the database. This can be
  done with GraphMan or via the PostgreSQL command line.
- `GRAPH_LOG`: control log levels, the same way that `RUST_LOG` is described
  [here](https://docs.rs/env_logger/0.6.0/env_logger/)
- `THEGRAPH_STORE_POSTGRES_DIESEL_URL`: postgres instance used when running
  tests. Set to `postgresql://<DBUSER>:<DBPASSWORD>@<DBHOST>:<DBPORT>/<DBNAME>`
- `GRAPH_KILL_IF_UNRESPONSIVE`: If set, the process will be killed if unresponsive.
- `GRAPH_KILL_IF_UNRESPONSIVE_TIMEOUT_SECS`: Timeout in seconds before killing
  the node if `GRAPH_KILL_IF_UNRESPONSIVE` is true. The default value is 10s.
- `GRAPH_LOG_QUERY_TIMING`: Control whether the process logs details of
  processing GraphQL and SQL queries. The value is a comma separated list
  of `sql`,`gql`, and `cache`. If `gql` is present in the list, each
  GraphQL query made against the node is logged at level `info`. The log
  message contains the subgraph that was queried, the query, its variables,
  the amount of time the query took, and a unique `query_id`. If `sql` is
  present, the SQL queries that a GraphQL query causes are logged. The log
  message contains the subgraph, the query, its bind variables, the amount
  of time it took to execute the query, the number of entities found by the
  query, and the `query_id` of the GraphQL query that caused the SQL
  query. These SQL queries are marked with `component: GraphQlRunner` There
  are additional SQL queries that get logged when `sql` is given. These are
  queries caused by mappings when processing blocks for a subgraph, and
  queries caused by subscriptions. If `cache` is present in addition to
  `gql`, also logs information for each toplevel GraphQL query field
  whether that could be retrieved from cache or not. Defaults to no
  logging.
- `GRAPH_LOG_TIME_FORMAT`: Custom log time format.Default value is `%b %d %H:%M:%S%.3f`. More information [here](https://docs.rs/chrono/latest/chrono/#formatting-and-parsing).
- `STORE_CONNECTION_POOL_SIZE`: How many simultaneous connections to allow to the store.
  Due to implementation details, this value may not be strictly adhered to. Defaults to 10.
- `GRAPH_LOG_POI_EVENTS`: Logs Proof of Indexing events deterministically.
  This may be useful for debugging.
- `GRAPH_LOAD_WINDOW_SIZE`, `GRAPH_LOAD_BIN_SIZE`: Load can be
  automatically throttled if load measurements over a time period of
  `GRAPH_LOAD_WINDOW_SIZE` seconds exceed a threshold. Measurements within
  each window are binned into bins of `GRAPH_LOAD_BIN_SIZE` seconds. The
  variables default to 300s and 1s
- `GRAPH_LOAD_THRESHOLD`: If wait times for getting database connections go
  above this threshold, throttle queries until the wait times fall below
  the threshold. Value is in milliseconds, and defaults to 0 which
  turns throttling and any associated statistics collection off.
- `GRAPH_LOAD_JAIL_THRESHOLD`: When the system is overloaded, any query
  that causes more than this fraction of the effort will be rejected for as
  long as the process is running (i.e., even after the overload situation
  is resolved) If this variable is not set, no queries will ever be jailed,
  but they will still be subject to normal load management when the system
  is overloaded.
- `GRAPH_LOAD_SIMULATE`: Perform all the steps that the load manager would
  given the other load management configuration settings, but never
  actually decline to run a query, instead log about load management
  decisions. Set to `true` to turn simulation on, defaults to `false`
- `GRAPH_STORE_CONNECTION_TIMEOUT`: How long to wait to connect to a
  database before assuming the database is down in ms. Defaults to 5000ms.
- `EXPERIMENTAL_SUBGRAPH_VERSION_SWITCHING_MODE`: default is `instant`, set
  to `synced` to only switch a named subgraph to a new deployment once it
  has synced, making the new deployment the "Pending" version.
- `GRAPH_REMOVE_UNUSED_INTERVAL`: How long to wait before removing an
  unused deployment. The system periodically checks and marks deployments
  that are not used by any subgraphs any longer. Once a deployment has been
  identified as unused, `graph-node` will wait at least this long before
  actually deleting the data (value is in minutes, defaults to 360, i.e. 6
  hours)
- `GRAPH_ALLOW_NON_DETERMINISTIC_IPFS`: enables indexing of subgraphs which
  use `ipfs.cat` as part of subgraph mappings. **This is an experimental
  feature which is not deterministic, and will be removed in future**.
- `GRAPH_STORE_BATCH_TARGET_DURATION`: How long batch operations during
  copying or grafting should take. This limits how long transactions for
  such long running operations will be, and therefore helps control bloat
  in other tables. Value is in seconds and defaults to 180s.
- `GRAPH_START_BLOCK`: block hash:block number where the forked subgraph will start indexing at.
- `GRAPH_FORK_BASE`: api url for where the graph node will fork from, use `https://api.thegraph.com/subgraphs/id/`
  for the hosted service.
- `GRAPH_DEBUG_FORK`: the IPFS hash id of the subgraph to fork.
- `GRAPH_STORE_HISTORY_SLACK_FACTOR`: How much history a subgraph with
  limited history can accumulate before it will be pruned. Setting this to
  1.1 means that the subgraph will be pruned every time it contains 10%
  more history (in blocks) than its history limit. The default value is 1.2
  and the value must be at least 1.01
- `GRAPH_STORE_HISTORY_REBUILD_THRESHOLD`,
  `GRAPH_STORE_HISTORY_DELETE_THRESHOLD`: when pruning, prune by copying
  the entities we will keep to new tables if we estimate that we will
  remove more than a factor of `REBUILD_THRESHOLD` of the deployment's
  history. If we estimate to remove a factor between `REBUILD_THRESHOLD`
  and `DELETE_THRESHOLD`, prune by deleting from the existing tables of the
  deployment. If we estimate to remove less than `DELETE_THRESHOLD`
  entities, do not change the table. Both settings are floats, and default
  to 0.5 for the `REBUILD_THRESHOLD` and 0.05 for the `DELETE_THRESHOLD`;
  they must be between 0 and 1, and `REBUILD_THRESHOLD` must be bigger than
  `DELETE_THRESHOLD`.
- `GRAPH_STORE_WRITE_BATCH_DURATION`: how long to accumulate changes during
  syncing into a batch before a write has to happen in seconds. The default
  is 300s. Setting this to 0 disables write batching.
- `GRAPH_STORE_WRITE_BATCH_SIZE`: how many changes to accumulate during
  syncing in kilobytes before a write has to happen. The default is 10_000
  which corresponds to 10MB. Setting this to 0 disables write batching.
