alter table
    subgraphs.subgraph_manifest
add
    column features text[] NOT NULL DEFAULT '{}';
