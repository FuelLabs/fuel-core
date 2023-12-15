alter table subgraphs.subgraph_deployment add unique (id);

-- create subgraph_error
create table subgraphs."subgraph_error" (
        "id"                 text not null,
        "subgraph_id"        text not null references subgraphs.subgraph_deployment(id) on delete cascade,
        "message"            text not null,
        "block_number"       numeric,
        "block_hash"         bytea,
        "handler"            text,

        vid                  bigserial primary key,
        block_range          int4range not null,
        exclude using gist   (id with =, block_range with &&)
);

create index attr_16_0_subgraph_error_id
    on subgraphs."subgraph_error" using btree("id");
create index attr_16_1_subgraph_error_subgraph_id
    on subgraphs."subgraph_error" using btree(left("subgraph_id", 256));
create index attr_16_3_subgraph_error_block_number
    on subgraphs."subgraph_error" using btree("block_number");
    
-- add fatal_error column to subgraph_deployment
alter table
    subgraphs.subgraph_deployment
add
    column fatal_error text;

-- add non_fatal_errors column to subgraph_deployment
alter table
    subgraphs.subgraph_deployment
add
    column non_fatal_errors text[] default '{}';

-- add health column to subgraph_deployment
create type subgraphs."health"
    as enum ('failed', 'healthy', 'unhealthy');

alter table
    subgraphs.subgraph_deployment
add
    column health subgraphs.health;

create index attr_subgraph_deployment_health
    on subgraphs."subgraph_deployment" using btree("health");

update
    subgraphs.subgraph_deployment
set
    health = case
        failed
        when false then 'healthy'::subgraphs.health
        when true then 'failed'::subgraphs.health
    end;

alter table
    subgraphs.subgraph_deployment
alter column
    health
set
    not null;
