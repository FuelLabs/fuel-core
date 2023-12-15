alter table deployment_schemas
      drop constraint deployment_schemas_subgraph_key;

create unique index deployment_schemas_subgraph_shard_uq
       on deployment_schemas(subgraph, shard);
