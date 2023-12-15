drop table chains;

alter table deployment_schemas
  drop constraint deployment_schemas_network_fkey;

alter table deployment_schemas
  add constraint deployment_schemas_network_fkey
  foreign key (network) references ethereum_networks(name);
