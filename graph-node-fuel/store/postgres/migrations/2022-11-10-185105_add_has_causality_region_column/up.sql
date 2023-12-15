alter table subgraphs.subgraph_manifest add column if not exists entities_with_causality_region text[] not null default array[]::text[];
