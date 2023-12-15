create table if not exists subgraphs.graph_node_versions (
    id serial primary key,
    git_commit_hash text not null,
    git_repository_dirty boolean not null,
    crate_version text not null,
    major integer not null,
    minor integer not null,
    patch integer not null,
    constraint unique_graph_node_versions unique (git_commit_hash, git_repository_dirty, crate_version, major, minor, patch)
);

alter table subgraphs.subgraph_manifest
    add column graph_node_version_id integer,
    add constraint graph_node_versions_fk foreign key (graph_node_version_id) references subgraphs.graph_node_versions (id);
