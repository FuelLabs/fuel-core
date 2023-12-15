alter table chains
  alter column net_version drop not null,
  alter column genesis_block_hash drop not null;

alter table ethereum_networks
  alter column net_version drop not null,
  alter column genesis_block_hash drop not null;
