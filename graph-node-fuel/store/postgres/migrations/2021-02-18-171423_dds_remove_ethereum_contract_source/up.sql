drop index subgraphs.attr_6_1_dynamic_ethereum_contract_data_source_kind;
drop index subgraphs.attr_6_2_dynamic_ethereum_contract_data_source_name;
drop index subgraphs.attr_6_3_dynamic_ethereum_contract_data_source_network;
drop index subgraphs.attr_6_7_dynamic_ethereum_contract_data_source_ethereum_block_h;
drop index subgraphs.attr_6_8_dynamic_ethereum_contract_data_source_ethereum_block_n;

update subgraphs.dynamic_ethereum_contract_data_source dds
   set address = ecs.address,
       abi = ecs.abi,
       start_block = ecs.start_block::int
  from subgraphs.ethereum_contract_source ecs
 where dds.source = ecs.id
   and dds.address is null;

alter table subgraphs.dynamic_ethereum_contract_data_source
      alter column address set not null,
      alter column abi     set not null,
      alter column start_block set not null;

alter table subgraphs.dynamic_ethereum_contract_data_source
      drop column source;
drop table subgraphs.ethereum_contract_source;
drop index subgraphs.dynamic_ethereum_contract_data_source_address;
