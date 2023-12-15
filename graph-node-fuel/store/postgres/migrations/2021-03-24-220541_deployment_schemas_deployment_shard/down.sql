drop constraint deployment_schemas_subgraph_shard_uq;

create unique index deployment_schemas_subgraph_key
       on deployment_schemas(subgraph);
