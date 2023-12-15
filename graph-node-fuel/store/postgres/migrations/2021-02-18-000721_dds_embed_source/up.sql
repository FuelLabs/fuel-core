alter table subgraphs.dynamic_ethereum_contract_data_source
      add column address     bytea,
      add column abi         text,
      add column start_block int;

-- We will use this index in a later migration to quickly identify
-- dds whose ethereum_contract_source has not been copied into the table
-- yet
create index dynamic_ethereum_contract_data_source_address on
  subgraphs.dynamic_ethereum_contract_data_source(address);
