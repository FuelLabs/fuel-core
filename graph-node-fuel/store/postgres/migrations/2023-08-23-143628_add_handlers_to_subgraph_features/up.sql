truncate table subgraphs.subgraph_features;

alter table
    subgraphs.subgraph_features
add
    column handlers text [] not null default '{}';