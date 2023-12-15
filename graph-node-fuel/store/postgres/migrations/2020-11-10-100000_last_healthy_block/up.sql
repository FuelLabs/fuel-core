alter table
    subgraphs.subgraph_deployment
add
    column last_healthy_ethereum_block_hash bytea;

alter table
    subgraphs.subgraph_deployment
add
    column last_healthy_ethereum_block_number numeric;
