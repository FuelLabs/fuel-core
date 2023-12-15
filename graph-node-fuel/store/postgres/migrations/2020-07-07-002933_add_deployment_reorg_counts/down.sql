alter table subgraphs.subgraph_deployment
  drop column reorg_count,
  drop column current_reorg_depth,
  drop column max_reorg_depth;
