alter table
    subgraphs.dynamic_ethereum_contract_data_source
add
    column templates;

alter table
    subgraphs.ethereum_contract_data_source
add
    column templates;
