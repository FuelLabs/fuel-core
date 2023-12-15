alter table subgraphs.subgraph_deployment
      add column earliest_ethereum_block_number numeric,
      add column earliest_ethereum_block_hash bytea;

update subgraphs.subgraph_deployment d
   set earliest_ethereum_block_number = m.start_block_number,
       earliest_ethereum_block_hash = m.start_block_hash
  from subgraphs.subgraph_manifest m
 where m.id = d.id;
