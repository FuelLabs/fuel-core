-- add `deterministic` column to subgraph_error
alter table
    subgraphs.subgraph_error
add
    column deterministic boolean not null default false;
