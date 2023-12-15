--
-- Generated with
--   cargo run --example layout -- -g drop-views \
--     ./store/postgres/src/subgraphs.graphql subgraphs
--

drop view if exists "subgraphs"."ethereum_contract_data_source_template";
drop view if exists "subgraphs"."ethereum_contract_data_source_template_source";
drop view if exists "subgraphs"."ethereum_contract_source";
drop view if exists "subgraphs"."subgraph_deployment_assignment";
drop view if exists "subgraphs"."subgraph";
drop view if exists "subgraphs"."dynamic_ethereum_contract_data_source";
drop view if exists "subgraphs"."subgraph_deployment";
drop view if exists "subgraphs"."subgraph_version";
drop view if exists "subgraphs"."ethereum_block_handler_entity";
drop view if exists "subgraphs"."ethereum_contract_abi";
drop view if exists "subgraphs"."ethereum_call_handler_entity";
drop view if exists "subgraphs"."ethereum_contract_data_source";
drop view if exists "subgraphs"."ethereum_contract_mapping";
drop view if exists "subgraphs"."subgraph_manifest";
drop view if exists "subgraphs"."ethereum_block_handler_filter_entity";
drop view if exists "subgraphs"."ethereum_contract_event_handler";
