alter table chains
  alter column net_version set not null,
  alter column genesis_block_hash set not null;

alter table ethereum_networks
  alter column net_version set not null,
  alter column genesis_block_hash set not null;
