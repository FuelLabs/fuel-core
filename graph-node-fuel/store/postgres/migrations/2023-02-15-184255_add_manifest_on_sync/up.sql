alter table subgraphs.subgraph_manifest
  add column on_sync text
  -- use a check constraint instead of an enum because
  -- enums are a pain to update
  constraint subgraph_manifest_on_sync_ck check (on_sync in ('activate', 'replace'));
