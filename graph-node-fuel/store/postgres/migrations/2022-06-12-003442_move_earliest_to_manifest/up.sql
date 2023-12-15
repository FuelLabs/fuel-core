alter table subgraphs.subgraph_manifest
      add column start_block_number int4,
      add column start_block_hash   bytea;

update subgraphs.subgraph_manifest m
   set start_block_number = d.earliest_ethereum_block_number,
       start_block_hash = d.earliest_ethereum_block_hash
  from subgraphs.subgraph_deployment d
 where m.id = d.id;

alter table subgraphs.subgraph_deployment
      add column earliest_block_number int4 not null default 0;

update subgraphs.subgraph_deployment
   set earliest_block_number = earliest_ethereum_block_number
 where earliest_ethereum_block_number is not null;
