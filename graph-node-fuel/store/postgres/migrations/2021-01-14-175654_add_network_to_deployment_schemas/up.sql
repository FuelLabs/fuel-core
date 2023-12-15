alter table deployment_schemas
  add column network text references ethereum_networks(name);

update deployment_schemas ds
   set network = d.network
  from subgraphs.subgraph_deployment_detail d
 where d.id = ds.subgraph;
