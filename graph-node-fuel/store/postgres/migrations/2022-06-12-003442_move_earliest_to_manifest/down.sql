alter table subgraphs.subgraph_deployment
      drop column earliest_block_number;

alter table subgraphs.subgraph_manifest
      drop column start_block_number,
      drop column start_block_hash;
