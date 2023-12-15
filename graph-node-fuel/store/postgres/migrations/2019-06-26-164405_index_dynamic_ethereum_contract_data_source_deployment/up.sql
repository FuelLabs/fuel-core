create index if not exists
  manual_dynamic_ethereum_contract_data_source_deployment
    on subgraphs.entities(((data->'deployment'->>'data')))
    where entity='DynamicEthereumContractDataSource';
