-- none of these indexes are needed for queries we run on subgraph_deployment
drop index if exists subgraphs.attr_2_0_subgraph_deployment_id;
drop index if exists subgraphs.attr_2_2_subgraph_deployment_failed;
drop index if exists subgraphs.attr_2_3_subgraph_deployment_synced;
drop index if exists
  subgraphs.attr_2_4_subgraph_deployment_earliest_ethereum_block_hash;
drop index if exists
  subgraphs.attr_2_5_subgraph_deployment_earliest_ethereum_block_number;
drop index if exists
  subgraphs.attr_2_6_subgraph_deployment_latest_ethereum_block_hash;
drop index if exists
  subgraphs.attr_2_7_subgraph_deployment_latest_ethereum_block_number;
drop index if exists subgraphs.attr_2_11_subgraph_deployment_entity_count;
drop index if exists subgraphs.attr_subgraph_deployment_health;

-- these indexes use the 'left' prefix of an attr, which we don't use in queries
drop index if exists
  subgraphs.attr_3_1_subgraph_deployment_assignment_node_id;
drop index if exists
  subgraphs.attr_16_1_subgraph_error_subgraph_id;
