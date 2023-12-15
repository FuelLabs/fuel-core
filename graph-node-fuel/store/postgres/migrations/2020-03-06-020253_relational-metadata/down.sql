-- Remove the tables used for relational storage of subgraph metadata
-- Everything between the comments 'BEGIN LAYOUT' and 'END LAYOUT' was generated
-- by running
--   cargo run --example layout -- -g migrate \
--     ./store/postgres/src/subgraphs.graphql subgraphs

-- BEGIN LAYOUT
drop table "subgraphs"."subgraph_manifest";
drop table "subgraphs"."ethereum_contract_source";
drop table "subgraphs"."ethereum_block_handler_entity";
drop table "subgraphs"."subgraph_version";
drop table "subgraphs"."ethereum_block_handler_filter_entity";
drop table "subgraphs"."ethereum_contract_abi";
drop table "subgraphs"."subgraph_deployment_assignment";
drop table "subgraphs"."dynamic_ethereum_contract_data_source";
drop table "subgraphs"."ethereum_contract_mapping";
drop table "subgraphs"."subgraph_deployment";
drop table "subgraphs"."subgraph";
drop table "subgraphs"."ethereum_contract_data_source_template_source";
drop table "subgraphs"."ethereum_call_handler_entity";
drop table "subgraphs"."ethereum_contract_data_source";
drop table "subgraphs"."ethereum_contract_event_handler";
drop table "subgraphs"."ethereum_contract_data_source_template";
-- END LAYOUT

update deployment_schemas
   set version = 'split'
 where name = 'subgraphs';
