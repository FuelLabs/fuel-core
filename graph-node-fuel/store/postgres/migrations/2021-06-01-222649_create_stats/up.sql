create table subgraphs.table_stats(
  id              serial primary key,
  deployment      int not null
                  references subgraphs.subgraph_deployment
                  on delete cascade,
  table_name      text not null,
  is_account_like bool,
  unique(deployment, table_name)
);
