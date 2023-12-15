alter table subgraphs.subgraph_deployment
  add column reorg_count int not null default 0,
  add column current_reorg_depth int not null default 0,
  add column max_reorg_depth int not null default 0;
