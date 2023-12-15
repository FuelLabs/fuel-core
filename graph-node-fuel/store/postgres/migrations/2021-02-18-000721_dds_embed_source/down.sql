alter table subgraphs.dynamic_ethereum_contract_data_source
      drop column address,
      drop column abi,
      drop column start_block;

drop index subgraphs.dynamic_ethereum_contract_data_source_address;
