alter table subgraphs.subgraph_manifest
      drop column data_sources,
      drop column templates;
drop table subgraphs.ethereum_contract_data_source;
drop table subgraphs.ethereum_contract_data_source_template;
drop table subgraphs.ethereum_contract_data_source_template_source;

alter table subgraphs.dynamic_ethereum_contract_data_source
      drop column mapping;
drop table subgraphs.ethereum_contract_mapping;
drop table subgraphs.ethereum_contract_abi;
drop table subgraphs.ethereum_block_handler_entity;
drop table subgraphs.ethereum_block_handler_filter_entity;
drop table subgraphs.ethereum_call_handler_entity;
drop table subgraphs.ethereum_contract_event_handler;
