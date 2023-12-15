--
-- Generated with
--   cargo run --example layout -- -g views \
--     ./store/postgres/src/subgraphs.graphql subgraphs
--

create view "subgraphs"."ethereum_contract_data_source_template" as
select id as id,
       (data->'kind'->>'data')::text as kind,
       (data->'name'->>'data')::text as name,
       (data->'network'->>'data')::text as network,
       (data->'source'->>'data')::text as source,
       (data->'mapping'->>'data')::text as mapping
  from subgraphs.entities
 where entity = 'EthereumContractDataSourceTemplate';

create view "subgraphs"."subgraph" as
select id as id,
       (data->'name'->>'data')::text as name,
       (data->'currentVersion'->>'data')::text as current_version,
       (data->'pendingVersion'->>'data')::text as pending_version,
       (data->'createdAt'->>'data')::numeric as created_at
  from subgraphs.entities
 where entity = 'Subgraph';

create view "subgraphs"."subgraph_version" as
select id as id,
       (data->'subgraph'->>'data')::text as subgraph,
       (data->'deployment'->>'data')::text as deployment,
       (data->'createdAt'->>'data')::numeric as created_at
  from subgraphs.entities
 where entity = 'SubgraphVersion';

create view "subgraphs"."ethereum_contract_data_source" as
select id as id,
       (data->'kind'->>'data')::text as kind,
       (data->'name'->>'data')::text as name,
       (data->'network'->>'data')::text as network,
       (data->'source'->>'data')::text as source,
       (data->'mapping'->>'data')::text as mapping,
       array(select (x->>'data')::text from jsonb_array_elements(data->'templates'->'data') x) as templates
  from subgraphs.entities
 where entity = 'EthereumContractDataSource';

create view "subgraphs"."dynamic_ethereum_contract_data_source" as
select id as id,
       (data->'kind'->>'data')::text as kind,
       (data->'name'->>'data')::text as name,
       (data->'network'->>'data')::text as network,
       (data->'source'->>'data')::text as source,
       (data->'mapping'->>'data')::text as mapping,
       array(select (x->>'data')::text from jsonb_array_elements(data->'templates'->'data') x) as templates,
       (data->'ethereumBlockHash'->>'data')::bytea as ethereum_block_hash,
       (data->'ethereumBlockNumber'->>'data')::numeric as ethereum_block_number,
       (data->'deployment'->>'data')::text as deployment
  from subgraphs.entities
 where entity = 'DynamicEthereumContractDataSource';

create view "subgraphs"."ethereum_call_handler_entity" as
select id as id,
       (data->'function'->>'data')::text as function,
       (data->'handler'->>'data')::text as handler
  from subgraphs.entities
 where entity = 'EthereumCallHandlerEntity';

create view "subgraphs"."ethereum_block_handler_filter_entity" as
select id as id,
       (data->'kind'->>'data')::text as kind
  from subgraphs.entities
 where entity = 'EthereumBlockHandlerFilterEntity';

create view "subgraphs"."ethereum_contract_event_handler" as
select id as id,
       (data->'event'->>'data')::text as event,
       (data->'topic0'->>'data')::bytea as topic_0,
       (data->'handler'->>'data')::text as handler
  from subgraphs.entities
 where entity = 'EthereumContractEventHandler';

create view "subgraphs"."subgraph_deployment_assignment" as
select id as id,
       (data->'nodeId'->>'data')::text as node_id,
       (data->'cost'->>'data')::numeric as cost
  from subgraphs.entities
 where entity = 'SubgraphDeploymentAssignment';

create view "subgraphs"."ethereum_block_handler_entity" as
select id as id,
       (data->'handler'->>'data')::text as handler,
       (data->'filter'->>'data')::text as filter
  from subgraphs.entities
 where entity = 'EthereumBlockHandlerEntity';

create view "subgraphs"."subgraph_deployment" as
select id as id,
       (data->'manifest'->>'data')::text as manifest,
       (data->'failed'->>'data')::boolean as failed,
       (data->'synced'->>'data')::boolean as synced,
       (data->'earliestEthereumBlockHash'->>'data')::bytea as earliest_ethereum_block_hash,
       (data->'earliestEthereumBlockNumber'->>'data')::numeric as earliest_ethereum_block_number,
       (data->'latestEthereumBlockHash'->>'data')::bytea as latest_ethereum_block_hash,
       (data->'latestEthereumBlockNumber'->>'data')::numeric as latest_ethereum_block_number,
       (data->'ethereumHeadBlockNumber'->>'data')::numeric as ethereum_head_block_number,
       (data->'ethereumHeadBlockHash'->>'data')::bytea as ethereum_head_block_hash,
       (data->'totalEthereumBlocksCount'->>'data')::numeric as total_ethereum_blocks_count,
       (data->'entityCount'->>'data')::numeric as entity_count
  from subgraphs.entities
 where entity = 'SubgraphDeployment';

create view "subgraphs"."subgraph_manifest" as
select id as id,
       (data->'specVersion'->>'data')::text as spec_version,
       (data->'description'->>'data')::text as description,
       (data->'repository'->>'data')::text as repository,
       (data->'schema'->>'data')::text as schema,
       array(select (x->>'data')::text from jsonb_array_elements(data->'dataSources'->'data') x) as data_sources,
       array(select (x->>'data')::text from jsonb_array_elements(data->'templates'->'data') x) as templates
  from subgraphs.entities
 where entity = 'SubgraphManifest';

create view "subgraphs"."ethereum_contract_mapping" as
select id as id,
       (data->'kind'->>'data')::text as kind,
       (data->'apiVersion'->>'data')::text as api_version,
       (data->'language'->>'data')::text as language,
       (data->'file'->>'data')::text as file,
       array(select (x->>'data')::text from jsonb_array_elements(data->'entities'->'data') x) as entities,
       array(select (x->>'data')::text from jsonb_array_elements(data->'abis'->'data') x) as abis,
       array(select (x->>'data')::text from jsonb_array_elements(data->'blockHandlers'->'data') x) as block_handlers,
       array(select (x->>'data')::text from jsonb_array_elements(data->'callHandlers'->'data') x) as call_handlers,
       array(select (x->>'data')::text from jsonb_array_elements(data->'eventHandlers'->'data') x) as event_handlers
  from subgraphs.entities
 where entity = 'EthereumContractMapping';

create view "subgraphs"."ethereum_contract_source" as
select id as id,
       (data->'address'->>'data')::text as address,
       (data->'abi'->>'data')::text as abi,
       (data->'startBlock'->>'data')::numeric as start_block
  from subgraphs.entities
 where entity = 'EthereumContractSource';

create view "subgraphs"."ethereum_contract_abi" as
select id as id,
       (data->'name'->>'data')::text as name,
       (data->'file'->>'data')::text as file
  from subgraphs.entities
 where entity = 'EthereumContractAbi';

create view "subgraphs"."ethereum_contract_data_source_template_source" as
select id as id,
       (data->'abi'->>'data')::text as abi
  from subgraphs.entities
 where entity = 'EthereumContractDataSourceTemplateSource';
