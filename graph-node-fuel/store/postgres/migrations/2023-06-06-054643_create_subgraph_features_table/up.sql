-- Creates a new table subgraph_features
create table if not exists subgraphs.subgraph_features (
  id text primary key,
  spec_version text not null,
  api_version text null,
  features text [] not null DEFAULT '{}',
  data_sources text [] not null DEFAULT '{}'
);