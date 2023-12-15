create table chains (
  id                 serial primary key,
  name               text not null unique,
  net_version        text,
  genesis_block_hash text,
  shard              text not null,

  constraint chains_genesis_version_check
    check (((net_version is null) = (genesis_block_hash is null)))
);

insert into chains(name, net_version, genesis_block_hash, shard)
select name, net_version, genesis_block_hash, 'primary'
  from ethereum_networks;

alter table deployment_schemas
  drop constraint deployment_schemas_network_fkey;

alter table deployment_schemas
  add constraint deployment_schemas_network_fkey
  foreign key (network) references chains(name);
