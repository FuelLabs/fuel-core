alter table subgraphs.subgraph_deployment
      drop column block_range;
alter table subgraphs.subgraph_deployment
      rename column id to deployment;
alter table subgraphs.subgraph_deployment
      add column id int;
