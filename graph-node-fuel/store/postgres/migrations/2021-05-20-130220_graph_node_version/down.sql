alter table subgraphs.subgraph_manifest
    drop constraint graph_node_versions_fk,
    drop column graph_node_version_id;

drop table if exists subgraphs.graph_node_versions;
