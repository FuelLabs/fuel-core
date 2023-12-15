alter table deployment_schemas
      add column active bool;

update deployment_schemas
   set active = true;

alter table deployment_schemas
      alter column active set not null;

-- we only allow one active entry per IPFS hash
create unique index deployment_schemas_deployment_active
       on deployment_schemas(subgraph) where active;
