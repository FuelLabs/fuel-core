update subgraphs.subgraph_deployment d
   set id = ds.id
  from deployment_schemas ds
 where ds.subgraph = d.deployment
   and d.id is null;

alter table subgraphs.subgraph_deployment
      drop constraint subgraph_deployment_pkey;

alter table subgraphs.subgraph_deployment
      add primary key(id);

alter table subgraphs.subgraph_deployment
      drop column vid;
