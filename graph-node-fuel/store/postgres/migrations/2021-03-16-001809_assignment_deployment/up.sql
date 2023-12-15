alter table subgraphs.subgraph_deployment_assignment
      add column new_id int;

update subgraphs.subgraph_deployment_assignment a
   set new_id = ds.id
  from deployment_schemas ds
 where ds.subgraph = a.id;

alter table subgraphs.subgraph_deployment_assignment
      drop column id,
      drop column vid,
      drop column block_range,
      drop column cost;

alter table subgraphs.subgraph_deployment_assignment
      rename column new_id to id;

alter table subgraphs.subgraph_deployment_assignment
      add primary key(id);
