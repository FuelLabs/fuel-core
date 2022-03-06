-- Your SQL goes here
create type ColumnTypeName as enum (
    'ID', 'Address', 'AssetId', 'Bytes4', 'Bytes8', 'Bytes32', 'ContractId', 'Salt', 'Blob'
);

create schema graph_registry;

create table graph_registry.type_ids (
    id bigint primary key,
    schema_version varchar(512) not null,
    schema_name varchar(32) not null,
    graphql_name varchar(32) not null,
    table_name varchar(32) not null
);

create table graph_registry.columns (
    id serial primary key,
    type_id bigint not null,
    column_position integer not null,
    column_name varchar(32) not null,
    column_type ColumnTypeName not null,
    nullable boolean not null,
    constraint fk_table_name
        foreign key(type_id)
            references graph_registry.type_ids(id)
);
