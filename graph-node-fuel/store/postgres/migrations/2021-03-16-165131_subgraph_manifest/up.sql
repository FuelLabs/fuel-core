alter table subgraphs.subgraph_manifest
      add column new_id int
          references subgraphs.subgraph_deployment(id) on delete cascade;

update subgraphs.subgraph_manifest m
   set new_id = d.id
  from subgraphs.subgraph_deployment d
 where d.manifest = m.id;

alter table subgraphs.subgraph_manifest
      drop column id,
      drop column vid,
      drop column block_range;

alter table subgraphs.subgraph_manifest
      rename column new_id to id;

alter table subgraphs.subgraph_manifest
      add primary key(id);

alter table subgraphs.subgraph_deployment
      drop column manifest;

drop index subgraphs.attr_4_1_subgraph_manifest_spec_version;
drop index subgraphs.attr_4_2_subgraph_manifest_description;
drop index subgraphs.attr_4_3_subgraph_manifest_repository;
drop index subgraphs.attr_4_4_subgraph_manifest_schema;
