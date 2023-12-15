alter table subgraphs.subgraph_manifest
  add column history_blocks int4
             not null default 2147483647
             check (history_blocks > 0);
