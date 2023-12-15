-- The relational schema for subgraph metadata
-- Everything between the comments 'BEGIN LAYOUT' and 'END LAYOUT' was generated
-- by running
--   cargo run --example layout -- -g migrate \
--     ./store/postgres/src/subgraphs.graphql subgraphs

--
-- Helpers to translate the entity data into the new form
--
create temporary table history_block_numbers as
select eb.number, eh.entity, eh.entity_id, eh.op_id
  from ethereum_blocks eb,
       subgraphs.entity_history eh,
       event_meta_data emd
 where eh.event_id = emd.id
   and emd.source = eb.hash;

create index history_entity on history_block_numbers(entity, entity_id);

-- We expect only op_id = 0 since these subgraph entities
-- are immutable; to cause an error when that is not the
-- case, we use a null id otherwise
create or replace view history_data as
select int4range(coalesce(h.number::int4, -1), null) as block_range,
       e.entity,
       case when coalesce(h.op_id,0) = 0 then e.id else null end as id,
       e.data
  from subgraphs.entities e
       left outer join history_block_numbers h
       on e.entity=h.entity and e.id = h.entity_id;

--
-- Generated DDL and data copying
--

-- BEGIN LAYOUT
drop view if exists "subgraphs"."ethereum_contract_data_source";
drop view if exists "subgraphs"."subgraph_deployment_assignment";
drop view if exists "subgraphs"."ethereum_block_handler_entity";
drop view if exists "subgraphs"."ethereum_contract_abi";
drop view if exists "subgraphs"."ethereum_block_handler_filter_entity";
drop view if exists "subgraphs"."subgraph_deployment";
drop view if exists "subgraphs"."ethereum_contract_data_source_template_source";
drop view if exists "subgraphs"."subgraph_version";
drop view if exists "subgraphs"."subgraph";
drop view if exists "subgraphs"."ethereum_call_handler_entity";
drop view if exists "subgraphs"."subgraph_manifest";
drop view if exists "subgraphs"."ethereum_contract_source";
drop view if exists "subgraphs"."ethereum_contract_mapping";
drop view if exists "subgraphs"."ethereum_contract_data_source_template";
drop view if exists "subgraphs"."ethereum_contract_event_handler";
drop view if exists "subgraphs"."dynamic_ethereum_contract_data_source";
create table subgraphs."subgraph" (
        "id"                 text not null,
        "name"               text not null,
        "current_version"    text,
        "pending_version"    text,
        "created_at"         numeric not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_0_0_subgraph_id
    on subgraphs."subgraph" using btree("id");
create index attr_0_1_subgraph_name
    on subgraphs."subgraph" using btree(left("name", 256));
create index attr_0_2_subgraph_current_version
    on subgraphs."subgraph" using btree("current_version");
create index attr_0_3_subgraph_pending_version
    on subgraphs."subgraph" using btree("pending_version");
create index attr_0_4_subgraph_created_at
    on subgraphs."subgraph" using btree("created_at");

create table subgraphs."subgraph_version" (
        "id"                 text not null,
        "subgraph"           text not null,
        "deployment"         text not null,
        "created_at"         numeric not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_1_0_subgraph_version_id
    on subgraphs."subgraph_version" using btree("id");
create index attr_1_1_subgraph_version_subgraph
    on subgraphs."subgraph_version" using btree("subgraph");
create index attr_1_2_subgraph_version_deployment
    on subgraphs."subgraph_version" using btree("deployment");
create index attr_1_3_subgraph_version_created_at
    on subgraphs."subgraph_version" using btree("created_at");

create table subgraphs."subgraph_deployment" (
        "id"                 text not null,
        "manifest"           text not null,
        "failed"             boolean not null,
        "synced"             boolean not null,
        "earliest_ethereum_block_hash" bytea,
        "earliest_ethereum_block_number" numeric,
        "latest_ethereum_block_hash" bytea,
        "latest_ethereum_block_number" numeric,
        "ethereum_head_block_number" numeric,
        "ethereum_head_block_hash" bytea,
        "total_ethereum_blocks_count" numeric not null,
        "entity_count"       numeric not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_2_0_subgraph_deployment_id
    on subgraphs."subgraph_deployment" using btree("id");
create index attr_2_1_subgraph_deployment_manifest
    on subgraphs."subgraph_deployment" using btree("manifest");
create index attr_2_2_subgraph_deployment_failed
    on subgraphs."subgraph_deployment" using btree("failed");
create index attr_2_3_subgraph_deployment_synced
    on subgraphs."subgraph_deployment" using btree("synced");
create index attr_2_4_subgraph_deployment_earliest_ethereum_block_hash
    on subgraphs."subgraph_deployment" using btree("earliest_ethereum_block_hash");
create index attr_2_5_subgraph_deployment_earliest_ethereum_block_number
    on subgraphs."subgraph_deployment" using btree("earliest_ethereum_block_number");
create index attr_2_6_subgraph_deployment_latest_ethereum_block_hash
    on subgraphs."subgraph_deployment" using btree("latest_ethereum_block_hash");
create index attr_2_7_subgraph_deployment_latest_ethereum_block_number
    on subgraphs."subgraph_deployment" using btree("latest_ethereum_block_number");
create index attr_2_8_subgraph_deployment_ethereum_head_block_number
    on subgraphs."subgraph_deployment" using btree("ethereum_head_block_number");
create index attr_2_9_subgraph_deployment_ethereum_head_block_hash
    on subgraphs."subgraph_deployment" using btree("ethereum_head_block_hash");
create index attr_2_10_subgraph_deployment_total_ethereum_blocks_count
    on subgraphs."subgraph_deployment" using btree("total_ethereum_blocks_count");
create index attr_2_11_subgraph_deployment_entity_count
    on subgraphs."subgraph_deployment" using btree("entity_count");

create table subgraphs."subgraph_deployment_assignment" (
        "id"                 text not null,
        "node_id"            text not null,
        "cost"               numeric not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_3_0_subgraph_deployment_assignment_id
    on subgraphs."subgraph_deployment_assignment" using btree("id");
create index attr_3_1_subgraph_deployment_assignment_node_id
    on subgraphs."subgraph_deployment_assignment" using btree(left("node_id", 256));
create index attr_3_2_subgraph_deployment_assignment_cost
    on subgraphs."subgraph_deployment_assignment" using btree("cost");

create table subgraphs."subgraph_manifest" (
        "id"                 text not null,
        "spec_version"       text not null,
        "description"        text,
        "repository"         text,
        "schema"             text not null,
        "data_sources"       text[] not null,
        "templates"          text[],

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_4_0_subgraph_manifest_id
    on subgraphs."subgraph_manifest" using btree("id");
create index attr_4_1_subgraph_manifest_spec_version
    on subgraphs."subgraph_manifest" using btree(left("spec_version", 256));
create index attr_4_2_subgraph_manifest_description
    on subgraphs."subgraph_manifest" using btree(left("description", 256));
create index attr_4_3_subgraph_manifest_repository
    on subgraphs."subgraph_manifest" using btree(left("repository", 256));
create index attr_4_4_subgraph_manifest_schema
    on subgraphs."subgraph_manifest" using btree(left("schema", 256));
create index attr_4_5_subgraph_manifest_data_sources
    on subgraphs."subgraph_manifest" using gin("data_sources");
create index attr_4_6_subgraph_manifest_templates
    on subgraphs."subgraph_manifest" using gin("templates");

create table subgraphs."ethereum_contract_data_source" (
        "id"                 text not null,
        "kind"               text not null,
        "name"               text not null,
        "network"            text,
        "source"             text not null,
        "mapping"            text not null,
        "templates"          text[],

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_5_0_ethereum_contract_data_source_id
    on subgraphs."ethereum_contract_data_source" using btree("id");
create index attr_5_1_ethereum_contract_data_source_kind
    on subgraphs."ethereum_contract_data_source" using btree(left("kind", 256));
create index attr_5_2_ethereum_contract_data_source_name
    on subgraphs."ethereum_contract_data_source" using btree(left("name", 256));
create index attr_5_3_ethereum_contract_data_source_network
    on subgraphs."ethereum_contract_data_source" using btree(left("network", 256));
create index attr_5_4_ethereum_contract_data_source_source
    on subgraphs."ethereum_contract_data_source" using btree("source");
create index attr_5_5_ethereum_contract_data_source_mapping
    on subgraphs."ethereum_contract_data_source" using btree("mapping");
create index attr_5_6_ethereum_contract_data_source_templates
    on subgraphs."ethereum_contract_data_source" using gin("templates");

create table subgraphs."dynamic_ethereum_contract_data_source" (
        "id"                 text not null,
        "kind"               text not null,
        "name"               text not null,
        "network"            text,
        "source"             text not null,
        "mapping"            text not null,
        "templates"          text[],
        "ethereum_block_hash" bytea not null,
        "ethereum_block_number" numeric not null,
        "deployment"         text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_6_0_dynamic_ethereum_contract_data_source_id
    on subgraphs."dynamic_ethereum_contract_data_source" using btree("id");
create index attr_6_1_dynamic_ethereum_contract_data_source_kind
    on subgraphs."dynamic_ethereum_contract_data_source" using btree(left("kind", 256));
create index attr_6_2_dynamic_ethereum_contract_data_source_name
    on subgraphs."dynamic_ethereum_contract_data_source" using btree(left("name", 256));
create index attr_6_3_dynamic_ethereum_contract_data_source_network
    on subgraphs."dynamic_ethereum_contract_data_source" using btree(left("network", 256));
create index attr_6_4_dynamic_ethereum_contract_data_source_source
    on subgraphs."dynamic_ethereum_contract_data_source" using btree("source");
create index attr_6_5_dynamic_ethereum_contract_data_source_mapping
    on subgraphs."dynamic_ethereum_contract_data_source" using btree("mapping");
create index attr_6_6_dynamic_ethereum_contract_data_source_templates
    on subgraphs."dynamic_ethereum_contract_data_source" using gin("templates");
create index attr_6_7_dynamic_ethereum_contract_data_source_ethereum_block_hash
    on subgraphs."dynamic_ethereum_contract_data_source" using btree("ethereum_block_hash");
create index attr_6_8_dynamic_ethereum_contract_data_source_ethereum_block_number
    on subgraphs."dynamic_ethereum_contract_data_source" using btree("ethereum_block_number");
create index attr_6_9_dynamic_ethereum_contract_data_source_deployment
    on subgraphs."dynamic_ethereum_contract_data_source" using btree("deployment");

create table subgraphs."ethereum_contract_source" (
        "id"                 text not null,
        "address"            bytea,
        "abi"                text not null,
        "start_block"        numeric,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_7_0_ethereum_contract_source_id
    on subgraphs."ethereum_contract_source" using btree("id");
create index attr_7_1_ethereum_contract_source_address
    on subgraphs."ethereum_contract_source" using btree("address");
create index attr_7_2_ethereum_contract_source_abi
    on subgraphs."ethereum_contract_source" using btree(left("abi", 256));
create index attr_7_3_ethereum_contract_source_start_block
    on subgraphs."ethereum_contract_source" using btree("start_block");

create table subgraphs."ethereum_contract_mapping" (
        "id"                 text not null,
        "kind"               text not null,
        "api_version"        text not null,
        "language"           text not null,
        "file"               text not null,
        "entities"           text[] not null,
        "abis"               text[] not null,
        "block_handlers"     text[],
        "call_handlers"      text[],
        "event_handlers"     text[],

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_8_0_ethereum_contract_mapping_id
    on subgraphs."ethereum_contract_mapping" using btree("id");
create index attr_8_1_ethereum_contract_mapping_kind
    on subgraphs."ethereum_contract_mapping" using btree(left("kind", 256));
create index attr_8_2_ethereum_contract_mapping_api_version
    on subgraphs."ethereum_contract_mapping" using btree(left("api_version", 256));
create index attr_8_3_ethereum_contract_mapping_language
    on subgraphs."ethereum_contract_mapping" using btree(left("language", 256));
create index attr_8_4_ethereum_contract_mapping_file
    on subgraphs."ethereum_contract_mapping" using btree(left("file", 256));
create index attr_8_5_ethereum_contract_mapping_entities
    on subgraphs."ethereum_contract_mapping" using gin("entities");
create index attr_8_6_ethereum_contract_mapping_abis
    on subgraphs."ethereum_contract_mapping" using gin("abis");
create index attr_8_7_ethereum_contract_mapping_block_handlers
    on subgraphs."ethereum_contract_mapping" using gin("block_handlers");
create index attr_8_8_ethereum_contract_mapping_call_handlers
    on subgraphs."ethereum_contract_mapping" using gin("call_handlers");
create index attr_8_9_ethereum_contract_mapping_event_handlers
    on subgraphs."ethereum_contract_mapping" using gin("event_handlers");

create table subgraphs."ethereum_contract_abi" (
        "id"                 text not null,
        "name"               text not null,
        "file"               text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_9_0_ethereum_contract_abi_id
    on subgraphs."ethereum_contract_abi" using btree("id");
create index attr_9_1_ethereum_contract_abi_name
    on subgraphs."ethereum_contract_abi" using btree(left("name", 256));
create index attr_9_2_ethereum_contract_abi_file
    on subgraphs."ethereum_contract_abi" using btree(left("file", 256));

create table subgraphs."ethereum_block_handler_entity" (
        "id"                 text not null,
        "handler"            text not null,
        "filter"             text,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_10_0_ethereum_block_handler_entity_id
    on subgraphs."ethereum_block_handler_entity" using btree("id");
create index attr_10_1_ethereum_block_handler_entity_handler
    on subgraphs."ethereum_block_handler_entity" using btree(left("handler", 256));
create index attr_10_2_ethereum_block_handler_entity_filter
    on subgraphs."ethereum_block_handler_entity" using btree("filter");

create table subgraphs."ethereum_block_handler_filter_entity" (
        "id"                 text not null,
        "kind"               text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_11_0_ethereum_block_handler_filter_entity_id
    on subgraphs."ethereum_block_handler_filter_entity" using btree("id");
create index attr_11_1_ethereum_block_handler_filter_entity_kind
    on subgraphs."ethereum_block_handler_filter_entity" using btree(left("kind", 256));

create table subgraphs."ethereum_call_handler_entity" (
        "id"                 text not null,
        "function"           text not null,
        "handler"            text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_12_0_ethereum_call_handler_entity_id
    on subgraphs."ethereum_call_handler_entity" using btree("id");
create index attr_12_1_ethereum_call_handler_entity_function
    on subgraphs."ethereum_call_handler_entity" using btree(left("function", 256));
create index attr_12_2_ethereum_call_handler_entity_handler
    on subgraphs."ethereum_call_handler_entity" using btree(left("handler", 256));

create table subgraphs."ethereum_contract_event_handler" (
        "id"                 text not null,
        "event"              text not null,
        "topic_0"            bytea,
        "handler"            text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_13_0_ethereum_contract_event_handler_id
    on subgraphs."ethereum_contract_event_handler" using btree("id");
create index attr_13_1_ethereum_contract_event_handler_event
    on subgraphs."ethereum_contract_event_handler" using btree(left("event", 256));
create index attr_13_2_ethereum_contract_event_handler_topic_0
    on subgraphs."ethereum_contract_event_handler" using btree("topic_0");
create index attr_13_3_ethereum_contract_event_handler_handler
    on subgraphs."ethereum_contract_event_handler" using btree(left("handler", 256));

create table subgraphs."ethereum_contract_data_source_template" (
        "id"                 text not null,
        "kind"               text not null,
        "name"               text not null,
        "network"            text,
        "source"             text not null,
        "mapping"            text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_14_0_ethereum_contract_data_source_template_id
    on subgraphs."ethereum_contract_data_source_template" using btree("id");
create index attr_14_1_ethereum_contract_data_source_template_kind
    on subgraphs."ethereum_contract_data_source_template" using btree(left("kind", 256));
create index attr_14_2_ethereum_contract_data_source_template_name
    on subgraphs."ethereum_contract_data_source_template" using btree(left("name", 256));
create index attr_14_3_ethereum_contract_data_source_template_network
    on subgraphs."ethereum_contract_data_source_template" using btree(left("network", 256));
create index attr_14_4_ethereum_contract_data_source_template_source
    on subgraphs."ethereum_contract_data_source_template" using btree("source");
create index attr_14_5_ethereum_contract_data_source_template_mapping
    on subgraphs."ethereum_contract_data_source_template" using btree("mapping");

create table subgraphs."ethereum_contract_data_source_template_source" (
        "id"                 text not null,
        "abi"                text not null,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);
create index attr_15_0_ethereum_contract_data_source_template_source_id
    on subgraphs."ethereum_contract_data_source_template_source" using btree("id");
create index attr_15_1_ethereum_contract_data_source_template_source_abi
    on subgraphs."ethereum_contract_data_source_template_source" using btree(left("abi", 256));


insert into
  "subgraphs"."ethereum_contract_data_source"(id, kind, name, network, source, mapping, templates, block_range)
select id,
       (data->'kind'->>'data')::text,
       (data->'name'->>'data')::text,
       (data->'network'->>'data')::text,
       (data->'source'->>'data')::text,
       (data->'mapping'->>'data')::text,
       array(select (x->>'data')::text from jsonb_array_elements(data->'templates'->'data') x),
      block_range
  from history_data
 where entity = 'EthereumContractDataSource';

insert into
  "subgraphs"."subgraph_deployment_assignment"(id, node_id, cost, block_range)
select id,
       (data->'nodeId'->>'data')::text,
       (data->'cost'->>'data')::numeric,
      block_range
  from history_data
 where entity = 'SubgraphDeploymentAssignment';

insert into
  "subgraphs"."ethereum_block_handler_entity"(id, handler, filter, block_range)
select id,
       (data->'handler'->>'data')::text,
       (data->'filter'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumBlockHandlerEntity';

insert into
  "subgraphs"."ethereum_contract_abi"(id, name, file, block_range)
select id,
       (data->'name'->>'data')::text,
       (data->'file'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumContractAbi';

insert into
  "subgraphs"."ethereum_block_handler_filter_entity"(id, kind, block_range)
select id,
       (data->'kind'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumBlockHandlerFilterEntity';

insert into
  "subgraphs"."subgraph_deployment"(id, manifest, failed, synced, earliest_ethereum_block_hash, earliest_ethereum_block_number, latest_ethereum_block_hash, latest_ethereum_block_number, ethereum_head_block_number, ethereum_head_block_hash, total_ethereum_blocks_count, entity_count, block_range)
select id,
       (data->'manifest'->>'data')::text,
       (data->'failed'->>'data')::boolean,
       (data->'synced'->>'data')::boolean,
       decode(replace(data->'earliestEthereumBlockHash'->>'data','0x',''),'hex'),
       (data->'earliestEthereumBlockNumber'->>'data')::numeric,
       decode(replace(data->'latestEthereumBlockHash'->>'data','0x',''),'hex'),
       (data->'latestEthereumBlockNumber'->>'data')::numeric,
       (data->'ethereumHeadBlockNumber'->>'data')::numeric,
       decode(replace(data->'ethereumHeadBlockHash'->>'data','0x',''),'hex'),
       (data->'totalEthereumBlocksCount'->>'data')::numeric,
       (data->'entityCount'->>'data')::numeric,
      block_range
  from history_data
 where entity = 'SubgraphDeployment';

insert into
  "subgraphs"."ethereum_contract_data_source_template_source"(id, abi, block_range)
select id,
       (data->'abi'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumContractDataSourceTemplateSource';

insert into
  "subgraphs"."subgraph_version"(id, subgraph, deployment, created_at, block_range)
select id,
       (data->'subgraph'->>'data')::text,
       (data->'deployment'->>'data')::text,
       (data->'createdAt'->>'data')::numeric,
      block_range
  from history_data
 where entity = 'SubgraphVersion';

insert into
  "subgraphs"."subgraph"(id, name, current_version, pending_version, created_at, block_range)
select id,
       (data->'name'->>'data')::text,
       (data->'currentVersion'->>'data')::text,
       (data->'pendingVersion'->>'data')::text,
       (data->'createdAt'->>'data')::numeric,
      block_range
  from history_data
 where entity = 'Subgraph';

insert into
  "subgraphs"."ethereum_call_handler_entity"(id, function, handler, block_range)
select id,
       (data->'function'->>'data')::text,
       (data->'handler'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumCallHandlerEntity';

insert into
  "subgraphs"."subgraph_manifest"(id, spec_version, description, repository, schema, data_sources, templates, block_range)
select id,
       (data->'specVersion'->>'data')::text,
       (data->'description'->>'data')::text,
       (data->'repository'->>'data')::text,
       (data->'schema'->>'data')::text,
       array(select (x->>'data')::text from jsonb_array_elements(data->'dataSources'->'data') x),
       array(select (x->>'data')::text from jsonb_array_elements(data->'templates'->'data') x),
      block_range
  from history_data
 where entity = 'SubgraphManifest';

insert into
  "subgraphs"."ethereum_contract_source"(id, address, abi, start_block, block_range)
select id,
       decode(replace(data->'address'->>'data','0x',''),'hex'),
       (data->'abi'->>'data')::text,
       (data->'startBlock'->>'data')::numeric,
      block_range
  from history_data
 where entity = 'EthereumContractSource';

insert into
  "subgraphs"."ethereum_contract_mapping"(id, kind, api_version, language, file, entities, abis, block_handlers, call_handlers, event_handlers, block_range)
select id,
       (data->'kind'->>'data')::text,
       (data->'apiVersion'->>'data')::text,
       (data->'language'->>'data')::text,
       (data->'file'->>'data')::text,
       array(select (x->>'data')::text from jsonb_array_elements(data->'entities'->'data') x),
       array(select (x->>'data')::text from jsonb_array_elements(data->'abis'->'data') x),
       array(select (x->>'data')::text from jsonb_array_elements(data->'blockHandlers'->'data') x),
       array(select (x->>'data')::text from jsonb_array_elements(data->'callHandlers'->'data') x),
       array(select (x->>'data')::text from jsonb_array_elements(data->'eventHandlers'->'data') x),
      block_range
  from history_data
 where entity = 'EthereumContractMapping';

insert into
  "subgraphs"."ethereum_contract_data_source_template"(id, kind, name, network, source, mapping, block_range)
select id,
       (data->'kind'->>'data')::text,
       (data->'name'->>'data')::text,
       (data->'network'->>'data')::text,
       (data->'source'->>'data')::text,
       (data->'mapping'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumContractDataSourceTemplate';

insert into
  "subgraphs"."ethereum_contract_event_handler"(id, event, topic_0, handler, block_range)
select id,
       (data->'event'->>'data')::text,
       decode(replace(data->'topic0'->>'data','0x',''),'hex'),
       (data->'handler'->>'data')::text,
      block_range
  from history_data
 where entity = 'EthereumContractEventHandler';

insert into
  "subgraphs"."dynamic_ethereum_contract_data_source"(id, kind, name, network, source, mapping, templates, ethereum_block_hash, ethereum_block_number, deployment, block_range)
select id,
       (data->'kind'->>'data')::text,
       (data->'name'->>'data')::text,
       (data->'network'->>'data')::text,
       (data->'source'->>'data')::text,
       (data->'mapping'->>'data')::text,
       array(select (x->>'data')::text from jsonb_array_elements(data->'templates'->'data') x),
       decode(replace(data->'ethereumBlockHash'->>'data','0x',''),'hex'),
       (data->'ethereumBlockNumber'->>'data')::numeric,
       (data->'deployment'->>'data')::text,
      block_range
  from history_data
 where entity = 'DynamicEthereumContractDataSource';
-- END LAYOUT

--
-- Final updates
--

update deployment_schemas
   set version = 'relational'
 where name = 'subgraphs';

drop view history_data;
drop table history_block_numbers;

-- Get rid of the old data
drop table subgraphs.entity_history;
drop table subgraphs.entities;
