alter table subgraphs.subgraph_deployment drop column health;
alter table subgraphs.subgraph_deployment drop column fatal_error;
alter table subgraphs.subgraph_deployment drop column non_fatal_errors;
drop type subgraph_health;
drop table subgraph_error;
