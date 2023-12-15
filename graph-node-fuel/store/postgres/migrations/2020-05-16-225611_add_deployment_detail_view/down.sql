alter table subgraphs.subgraph_deployment
  add column ethereum_head_block_hash bytea;
alter table subgraphs.subgraph_deployment
  add column ethereum_head_block_number numeric;
alter table subgraphs.subgraph_deployment
  add column total_ethereum_blocks_count numeric;

update subgraphs.subgraph_deployment sd
   set ethereum_head_block_hash = sdd.ethereum_head_block_hash,
       ethereum_head_block_number = sdd.ethereum_head_block_number,
       total_ethereum_blocks_count = sdd.ethereum_head_block_number
  from subgraphs.subgraph_deployment_detail sdd
 where sd.id = sdd.id;

drop view subgraphs.subgraph_deployment_detail;
