alter table subgraphs.subgraph_manifest
      add column use_bytea_prefix bool not null default 'f';
