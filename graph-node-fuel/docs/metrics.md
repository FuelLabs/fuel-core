graph-node provides the following metrics via Prometheus endpoint on 8040 port by default:
- `deployment_block_processing_duration`
Measures **duration of block processing** for a subgraph deployment
- `deployment_block_trigger_count`
Measures the **number of triggers in each** block for a subgraph deployment
- `deployment_count` 
Counts the number of deployments currently being indexed by the graph-node.
- `deployment_eth_rpc_errors`
Counts **eth** **rpc request errors** for a subgraph deployment
- `deployment_eth_rpc_request_duration`
Measures **eth** **rpc request duration** for a subgraph deployment
- `deployment_failed`
Boolean gauge to indicate **whether the deployment has failed** (1 == failed)
- `deployment_handler_execution_time`
Measures the **execution time for handlers**
- `deployment_head`
Track the **head block number** for a deployment. Example:

```protobuf
deployment_head{deployment="QmaeWFYbPwmXEk7UuACmkqgPq2Pba5t2RYdJtEyvAUmrxg",network="mumbai",shard="primary"} 19509077
```

- `deployment_host_fn_execution_time`
Measures the **execution time for host functions**
- `deployment_reverted_blocks`
Track the **last reverted block** for a subgraph deployment
- `deployment_sync_secs`
total **time spent syncing**
- `deployment_transact_block_operations_duration`
Measures **duration of committing all the entity operations** in a block and **updating the subgraph pointer**
- `deployment_trigger_processing_duration`
Measures **duration of trigger processing** for a subgraph deployment
- `eth_rpc_errors`
Counts **eth rpc request errors**
- `eth_rpc_request_duration`
Measures **eth rpc request duration**
- `ethereum_chain_head_number`
Block **number of the most recent block synced from Ethereum**. Example:

```protobuf
ethereum_chain_head_number{network="mumbai"} 20045294
```

- `metrics_register_errors`
Counts **Prometheus metrics register errors**
- `metrics_unregister_errors`
Counts **Prometheus metrics unregister errors**
- `query_cache_status_count`
Count **toplevel GraphQL fields executed** and their cache status
- `query_effort_ms`
Moving **average of time spent running queries**
- `query_execution_time`
**Execution time for successful GraphQL queries**
- `query_result_max`
the **maximum size of a query result** (in CacheWeight)
- `query_result_size` 
the **size of the result of successful GraphQL queries** (in CacheWeight)
- `query_semaphore_wait_ms`
Moving **average of time spent on waiting for postgres query semaphore**
- `query_blocks_behind`
A histogram for how many blocks behind the subgraph head queries are being made at.
This helps inform pruning decisions.
- `query_kill_rate`
The rate at which the load manager kills queries
- `registered_metrics`
Tracks the **number of registered metrics** on the node
- `store_connection_checkout_count`
The **number of Postgres connections** currently **checked out**
- `store_connection_error_count`
The **number of Postgres connections errors**
- `store_connection_wait_time_ms`
**Average connection wait time**